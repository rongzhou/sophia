//! 解释器：执行起步子集的 action / transition body。
//!
//! 见 docs/language_implementation.md 9.2、第十六节。解释器消费语法层 AST 与
//! Semantic 声明模型（entity / state / error / storage 元信息），直接执行，不经过
//! 中间语言。body 子语言（设计第七节）：let / set / return / raise / if-else /
//! match / repeat / print 与受限表达式。
//!
//! 控制流：`return` / `raise` 通过 [`Signal`] 提前结束块的执行；正常顺序执行用
//! `Signal::Next` 表示。`raise` 不是 Rust 错误，而是领域错误结果（[`RaisedError`]）；
//! 跨调用边界的 raise 经 `RuntimeError::Raised` 内部通道冒泡，在 `run` 边界物化为
//! [`Outcome::Raised`]。

use crate::error::RuntimeError;
use crate::host::HostRegistry;
use crate::trace::{SpanOutcome, Trace};
use crate::value::{RaisedError, Value};
use sophia_exec_ir::ExecGraph;
use sophia_semantic::SemanticModel;
use sophia_syntax::{Ast, BinOp, Block, Callable, ElseBranch, Expr, ExprId, Item, Pattern, Stmt};
use std::collections::BTreeMap;

/// action / transition 的执行结果：正常返回值或领域错误。
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// 正常 `return`。
    Returned(Value),
    /// `raise` 的领域错误。
    Raised(RaisedError),
}

/// 块内控制流信号。
enum Signal {
    /// 顺序执行到块尾，未提前终止。
    Next,
    /// `return expr`。
    Return(Value),
    /// `raise Variant { ... }`。
    Raise(RaisedError),
}

/// 变量环境（词法作用域栈）。
struct Env {
    frames: Vec<BTreeMap<String, Value>>,
}

impl Env {
    fn new() -> Self {
        Env {
            frames: vec![BTreeMap::new()],
        }
    }
    fn push(&mut self) {
        self.frames.push(BTreeMap::new());
    }
    fn pop(&mut self) {
        self.frames.pop();
    }
    fn declare(&mut self, name: impl Into<String>, v: Value) {
        self.frames.last_mut().unwrap().insert(name.into(), v);
    }
    fn lookup(&self, name: &str) -> Option<&Value> {
        self.frames.iter().rev().find_map(|f| f.get(name))
    }
    /// 更新已存在变量（`set`）：在最内含有该名字的层更新。
    fn assign(&mut self, name: &str, v: Value) -> bool {
        for frame in self.frames.iter_mut().rev() {
            if frame.contains_key(name) {
                frame.insert(name.to_string(), v);
                return true;
            }
        }
        false
    }
}

/// 解释器：借用 Semantic 模型、全程序 AST 集合与 effect 宿主。
///
/// 持有全部 AST（而非单个）：起步子集遵循「一文件一节点」，跨 action 调用必然
/// 跨 AST，因此 callable 查找需覆盖整个程序。`cur_ast` 指向当前正在执行的
/// callable 所属 AST——`ExprId` 仅在其所属 AST 的 arena 内有效，递归调用时
/// 由 [`Self::run`] 保存 / 恢复。
pub struct Interpreter<'a> {
    model: &'a SemanticModel,
    asts: &'a [&'a Ast],
    /// Execution Graph IR：设计 9.2 流水线 `Semantic IR → Execution Graph IR →
    /// Interpreter` 的桥梁。每次 callable 调用经此图解析（节点存在 + 调用边），
    /// 而非直接绕过图执行。
    graph: ExecGraph,
    cur_ast: &'a Ast,
    /// 当前正在执行的 callable 名（调用边的 from 端，用于经图校验调用）。
    cur_callable: String,
    /// 当前调用深度（顶层入口为 0，每层调用 +1）——trace span 的 depth。
    depth: u32,
    /// 执行 Trace：Execution Graph 执行的投影（9.4），记录每次 callable 进入。
    trace: Trace,
    /// effect 宿主注册表（路线 B）：`Lib.Op(args)` 经 `(family, op)` 委派给注册的 host；
    /// `print` 经 `console_write` 捕获。runtime 不内置具体库——库 host 由上层注册。
    host: &'a mut HostRegistry,
}

impl<'a> Interpreter<'a> {
    pub fn new(model: &'a SemanticModel, asts: &'a [&'a Ast], host: &'a mut HostRegistry) -> Self {
        let cur_ast = asts.first().copied().unwrap_or_else(|| {
            panic!("解释器至少需要一个 AST");
        });
        // 构建 Execution Graph IR（节点 + 调用边），作为执行的桥梁。
        let graph = ExecGraph::from_model(model, asts);
        Interpreter {
            model,
            asts,
            graph,
            cur_ast,
            cur_callable: String::new(),
            depth: 0,
            trace: Trace::new(),
            host,
        }
    }

    /// 取走本次执行收集的 Trace（消费 interpreter）。
    pub fn into_trace(self) -> Trace {
        self.trace
    }

    /// 执行某 action / transition，传入按 input 参数顺序的实参。
    ///
    /// 调用方负责实参顺序与数量正确（CLI / 上层 harness）。执行前对实参做
    /// runtime input validation（step 5），执行后对返回值做 output validation。
    pub fn run(&mut self, name: &str, args: Vec<Value>) -> Result<Outcome, RuntimeError> {
        // 经 Execution Graph IR 解析执行入口（设计 9.2 桥梁）：节点必须存在。
        if !self.graph.has_node(name) {
            return Err(RuntimeError::Validation(format!(
                "`{name}` 在 Execution Graph 中无执行节点"
            )));
        }
        // 若由另一 callable 调用（非顶层入口），调用边必须在图中（由 body 扫描物化）。
        if !self.cur_callable.is_empty() && !self.graph.has_call_edge(&self.cur_callable, name) {
            return Err(RuntimeError::Validation(format!(
                "`{}` → `{name}` 的调用未在 Execution Graph 中物化",
                self.cur_callable
            )));
        }

        // trace 投影（9.4）：解析本次进入对应的图节点与触发它的调用边（顶层入口无入边）。
        let node_id = self
            .graph
            .node_id_by_name(name)
            .expect("has_node 已校验节点存在");
        let edge_id = if self.cur_callable.is_empty() {
            None
        } else {
            self.graph.call_edge_id(&self.cur_callable, name)
        };
        let span_idx = self.trace.open(node_id, edge_id, name, self.depth);

        let (owner_ast, callable) = self
            .find_callable(name)
            .ok_or_else(|| RuntimeError::Validation(format!("未知 action `{name}`")))?;
        let decl = self
            .model
            .callables
            .get(name)
            .ok_or_else(|| RuntimeError::Validation(format!("`{name}` 不在语义模型中")))?;

        // runtime input validation：实参数量与类型须匹配 input 声明。
        if args.len() != decl.inputs.len() {
            return Err(RuntimeError::Validation(format!(
                "`{name}` 期望 {} 个实参，得到 {}",
                decl.inputs.len(),
                args.len()
            )));
        }
        let mut env = Env::new();
        for ((pname, pty), arg) in decl.inputs.iter().zip(args) {
            crate::validate::check_value(&arg, pty, self.model)
                .map_err(|e| RuntimeError::Validation(format!("input `{pname}`：{e}")))?;
            env.declare(pname.clone(), arg);
        }

        let body = callable
            .body
            .as_ref()
            .ok_or_else(|| RuntimeError::Validation(format!("`{name}` 无 body，无法解释执行")))?;

        // 切换当前 AST 与当前 callable（ExprId 仅在所属 AST 内有效；cur_callable 是
        // 调用边的 from 端），执行后恢复，支持递归 / 跨文件调用。深度同步 +1 / 恢复。
        let prev_ast = self.cur_ast;
        let prev_callable = std::mem::replace(&mut self.cur_callable, name.to_string());
        self.cur_ast = owner_ast;
        self.depth += 1;
        let exec_result = self.exec_block(body, &mut env);
        self.depth -= 1;
        self.cur_ast = prev_ast;
        self.cur_callable = prev_callable;

        let outcome = match exec_result {
            Ok(Signal::Return(v)) => Outcome::Returned(v),
            Ok(Signal::Raise(e)) => Outcome::Raised(e),
            Ok(Signal::Next) => {
                // 全路径 return/raise 由编译期保证；走到这说明是 Unit action 顺序结束。
                Outcome::Returned(Value::Unit)
            }
            // 被调用方在表达式求值中 raise 的领域错误经错误通道冒泡至此：
            // 在本调用边界物化为领域结果（error 传播由 semantic 层保证调用方已声明）。
            Err(RuntimeError::Raised(e)) => Outcome::Raised(e),
            Err(other) => return Err(other),
        };

        // trace 投影：写回本次进入的真实结局（与 span_idx 配对，9.4）。
        let span_outcome = match &outcome {
            Outcome::Returned(_) => SpanOutcome::Returned,
            Outcome::Raised(_) => SpanOutcome::Raised,
        };
        self.trace.close(span_idx, span_outcome);

        // runtime output validation：返回值须匹配 sole output 声明类型。
        if let Outcome::Returned(v) = &outcome {
            if let Some(out_ty) = decl.sole_output_ty() {
                crate::validate::check_value(v, out_ty, self.model)
                    .map_err(|e| RuntimeError::Validation(format!("output：{e}")))?;
            }
        }
        Ok(outcome)
    }

    fn find_callable(&self, name: &str) -> Option<(&'a Ast, &'a Callable)> {
        self.asts.iter().find_map(|&ast| {
            ast.items.iter().find_map(|it| match it {
                Item::Action(c) | Item::Transition(c) if c.name.text == name => Some((ast, c)),
                _ => None,
            })
        })
    }

    // ---- 语句 / 块执行 ----

    fn exec_block(&mut self, block: &Block, env: &mut Env) -> Result<Signal, RuntimeError> {
        env.push();
        let mut result = Signal::Next;
        for stmt in &block.stmts {
            match self.exec_stmt(stmt, env)? {
                Signal::Next => {}
                term => {
                    result = term;
                    break;
                }
            }
        }
        env.pop();
        Ok(result)
    }

    fn exec_stmt(&mut self, stmt: &Stmt, env: &mut Env) -> Result<Signal, RuntimeError> {
        match stmt {
            Stmt::Let { name, value, .. } => {
                let v = self.eval(*value, env)?;
                env.declare(name.text.clone(), v);
                Ok(Signal::Next)
            }
            Stmt::Set { name, value, .. } => {
                let v = self.eval(*value, env)?;
                if !env.assign(&name.text, v) {
                    return Err(RuntimeError::Validation(format!(
                        "set 了未声明的变量 `{}`",
                        name.text
                    )));
                }
                Ok(Signal::Next)
            }
            Stmt::Return { value, .. } => {
                let v = self.eval(*value, env)?;
                Ok(Signal::Return(v))
            }
            Stmt::Raise { value, .. } => {
                let e = self.eval_raise(*value, env)?;
                Ok(Signal::Raise(e))
            }
            Stmt::Print { value, .. } => {
                let v = self.eval(*value, env)?;
                self.host.console_write(&v.to_string());
                Ok(Signal::Next)
            }
            Stmt::Expr { value, .. } => {
                self.eval(*value, env)?;
                Ok(Signal::Next)
            }
            Stmt::If {
                condition,
                consequence,
                alternative,
                ..
            } => {
                let cond = self.eval(*condition, env)?;
                if cond.as_bool().unwrap_or(false) {
                    self.exec_block(consequence, env)
                } else {
                    match alternative {
                        Some(ElseBranch::Block(b)) => self.exec_block(b, env),
                        Some(ElseBranch::If(s)) => self.exec_stmt(s, env),
                        None => Ok(Signal::Next),
                    }
                }
            }
            Stmt::Match { subject, arms, .. } => {
                let subj = self.eval(*subject, env)?;
                for arm in arms {
                    if let Some(bindings) = match_pattern(&arm.pattern, &subj) {
                        env.push();
                        for (n, v) in bindings {
                            env.declare(n, v);
                        }
                        let sig = self.exec_block(&arm.body, env);
                        env.pop();
                        return sig;
                    }
                }
                // 编译期穷尽性保证不会走到这；防御性返回 Next。
                Ok(Signal::Next)
            }
            Stmt::Repeat { count, body, .. } => {
                let n = self.eval(*count, env)?.as_int().unwrap_or(0);
                for _ in 0..n.max(0) {
                    match self.exec_block(body, env)? {
                        Signal::Next => {}
                        term => return Ok(term),
                    }
                }
                Ok(Signal::Next)
            }
        }
    }

    /// 求值 `raise` 的构造表达式为领域错误。
    fn eval_raise(&mut self, id: ExprId, env: &mut Env) -> Result<RaisedError, RuntimeError> {
        if let Expr::Construct { name, fields, .. } = self.cur_ast.expr(id) {
            let mut fvals = BTreeMap::new();
            for fi in fields {
                fvals.insert(fi.name.text.clone(), self.eval(fi.value, env)?);
            }
            Ok(RaisedError {
                variant: name.text.clone(),
                fields: fvals,
            })
        } else {
            Err(RuntimeError::Validation(
                "raise 的值不是 variant 构造".into(),
            ))
        }
    }

    // ---- 表达式求值 ----

    fn eval(&mut self, id: ExprId, env: &mut Env) -> Result<Value, RuntimeError> {
        let expr = self.cur_ast.expr(id);
        match expr {
            Expr::Str(s) => Ok(Value::Text(s.value.clone())),
            Expr::Int { text, .. } => text
                .parse::<i64>()
                .map(Value::Int)
                .map_err(|_| RuntimeError::Validation(format!("非法整数字面量 `{text}`"))),
            Expr::Bool { value, .. } => Ok(Value::Bool(*value)),
            Expr::Null { .. } => Ok(Value::Null),
            Expr::Ident(name) => env
                .lookup(&name.text)
                .cloned()
                .ok_or_else(|| RuntimeError::Validation(format!("未绑定变量 `{}`", name.text))),
            Expr::List { items, .. } => {
                let mut vals = Vec::with_capacity(items.len());
                for &it in items {
                    vals.push(self.eval(it, env)?);
                }
                Ok(Value::List(vals))
            }
            Expr::Field { base, field, .. } => {
                // 状态值访问：`StateName.Value`（base 是命名 state 的标识符）。
                if let Expr::Ident(bident) = self.cur_ast.expr(*base) {
                    if let Some(state) = self.model.states.get(&bident.text) {
                        if state.has_value(&field.text) {
                            return Ok(Value::State {
                                state: bident.text.clone(),
                                value: field.text.clone(),
                            });
                        }
                    }
                }
                let b = self.eval(*base, env)?;
                self.eval_field(&b, &field.text)
            }
            Expr::MethodCall {
                base, method, args, ..
            } => {
                // body 级库 op：`File.Read/Write(...)` / `Http.Get(url)` / 三方库 op
                // （特殊根 method_call）。
                if let Some(result) = self.try_effect_op(*base, &method.text, args, env)? {
                    return Ok(result);
                }
                let b = self.eval(*base, env)?;
                let mut argv = Vec::new();
                for &a in args {
                    argv.push(self.eval(a, env)?);
                }
                self.eval_method(b, &method.text, argv)
            }
            Expr::Call { callee, args, .. } => {
                let mut argv = Vec::new();
                for &a in args {
                    argv.push(self.eval(a, env)?);
                }
                self.eval_call(&callee.text, argv)
            }
            Expr::Construct { name, fields, .. } => {
                let mut fvals = BTreeMap::new();
                for fi in fields {
                    fvals.insert(fi.name.text.clone(), self.eval(fi.value, env)?);
                }
                self.eval_construct(&name.text, fvals)
            }
            Expr::Not { operand, .. } => {
                let v = self.eval(*operand, env)?;
                Ok(Value::Bool(!v.as_bool().unwrap_or(false)))
            }
            Expr::Neg { operand, .. } => {
                let v = self.eval(*operand, env)?;
                let i = v
                    .as_int()
                    .ok_or_else(|| RuntimeError::Validation("取负需要 Int".into()))?;
                Ok(Value::Int(-i))
            }
            Expr::Binary {
                op, left, right, ..
            } => {
                let l = self.eval(*left, env)?;
                let r = self.eval(*right, env)?;
                self.eval_binary(*op, l, r)
            }
        }
    }

    /// 字段 / 伪字段访问。
    fn eval_field(&self, base: &Value, field: &str) -> Result<Value, RuntimeError> {
        match base {
            Value::Text(s) if field == "length" => Ok(Value::Int(s.chars().count() as i64)),
            Value::Entity { fields, name } => fields.get(field).cloned().ok_or_else(|| {
                RuntimeError::Validation(format!("entity `{name}` 无字段 `{field}`"))
            }),
            // state value 的隐式访问（如 qualified base）在 build 时已被解析为 State 值。
            _ => Err(RuntimeError::Validation(format!(
                "类型 {base} 无字段 `{field}`"
            ))),
        }
    }

    /// 方法调用（起步子集：`list.append(item)`）。
    fn eval_method(
        &self,
        base: Value,
        method: &str,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        match (base, method) {
            (Value::List(mut items), "append") => {
                if let Some(item) = args.into_iter().next() {
                    items.push(item);
                }
                Ok(Value::List(items))
            }
            (b, m) => Err(RuntimeError::Validation(format!(
                "不支持的方法调用 `{m}`（在 {b} 上）"
            ))),
        }
    }

    /// 尝试求值 body 级库 effect op：`Lib.Op(args)`（特殊根 method_call，如 `File.Read(path)` /
    /// `Http.Get(url)`，见 docs/stdlib_design.md）。
    ///
    /// 仅当 `base` 是标识符且 `(family, method)` 存在于语义模型的库 op 契约时返回 `Some(结果)`，
    /// 否则 `None`（交回常规方法）。经 [`HostRegistry`] 按 `(family, op)` 委派——runtime 不认识
    /// 具体库，库的 host（标准库 native / mock、三方 WASM）由上层注册。若调用方漏注册 host，
    /// [`HostRegistry::call`] 会返回“无 host 实现”的诚实硬错误，不退回普通 method 路径。
    /// 取回的文本为 `Value::Text`（运行时不携带 intent 标签——intent 是编译期静态属性）。host
    /// 失败时返回 `Err`，物化为 `RuntimeError`（硬错误阻断，绝不伪造成功）。
    fn try_effect_op(
        &mut self,
        base: ExprId,
        method: &str,
        args: &[ExprId],
        env: &mut Env,
    ) -> Result<Option<Value>, RuntimeError> {
        // base 必须是标识符（库特殊根 family），且该 (family, op) 是语义模型中的已知库 op。
        let Expr::Ident(root) = self.cur_ast.expr(base) else {
            return Ok(None);
        };
        let family = root.text.clone();
        if self.model.library_op(&family, method).is_none() {
            return Ok(None);
        }

        // 求值实参，委派给 host（按 (family, op)）。缺 host 由 HostRegistry::call 诚实报错。
        let mut argv = Vec::with_capacity(args.len());
        for &a in args {
            argv.push(self.eval(a, env)?);
        }
        let result = self
            .host
            .call(&family, method, &argv)
            .map_err(RuntimeError::Validation)?;
        Ok(Some(result))
    }

    /// 调用：内置 `to_text` 或其他 action / transition。
    fn eval_call(&mut self, callee: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
        if callee == "to_text" {
            let v = args
                .into_iter()
                .next()
                .ok_or_else(|| RuntimeError::Validation("to_text 缺参数".into()))?;
            return Ok(Value::Text(match v {
                Value::Int(i) => i.to_string(),
                other => other.to_string(),
            }));
        }
        // 调用其他 callable：递归解释。被调用方 raise 的领域错误经
        // `RuntimeError::Raised` 通道向上冒泡，在最近的 `run` 边界物化为
        // `Outcome::Raised`（error 传播由 semantic 层保证调用方已声明该 variant）。
        match self.run(callee, args)? {
            Outcome::Returned(v) => Ok(v),
            Outcome::Raised(e) => Err(RuntimeError::Raised(e)),
        }
    }

    /// entity 构造或 transition 调用（构造式语法二义）。
    fn eval_construct(
        &mut self,
        name: &str,
        fields: BTreeMap<String, Value>,
    ) -> Result<Value, RuntimeError> {
        if self.model.entities.contains_key(name) {
            return Ok(Value::Entity {
                name: name.to_string(),
                fields,
            });
        }
        // transition 调用：以字段为命名实参，按 transition input 顺序重排后执行。
        if let Some(decl) = self.model.callables.get(name) {
            let args: Vec<Value> = decl
                .inputs
                .iter()
                .map(|(pname, _)| fields.get(pname).cloned().unwrap_or(Value::Unit))
                .collect();
            return match self.run(name, args)? {
                Outcome::Returned(v) => Ok(v),
                Outcome::Raised(e) => Err(RuntimeError::Raised(e)),
            };
        }
        // error variant 被**返回**（作为 `one of` 成员，区别于 raise）：构造 ErrorValue。
        if self.model.variants.contains_key(name) {
            return Ok(Value::ErrorValue {
                variant: name.to_string(),
                fields,
            });
        }
        // 可能是 qualified state value（如 `TodoStatus.Done`）误入构造路径：不应发生
        //（grammar 区分 qualified_name），防御性报错。
        Err(RuntimeError::Validation(format!("未知构造目标 `{name}`")))
    }

    fn eval_binary(&self, op: BinOp, l: Value, r: Value) -> Result<Value, RuntimeError> {
        use BinOp::*;
        let res = match op {
            And => Value::Bool(l.as_bool().unwrap_or(false) && r.as_bool().unwrap_or(false)),
            Or => Value::Bool(l.as_bool().unwrap_or(false) || r.as_bool().unwrap_or(false)),
            Eq => Value::Bool(values_equal(&l, &r)),
            Ne => Value::Bool(!values_equal(&l, &r)),
            Lt | Le | Gt | Ge => {
                let (a, b) = (
                    l.as_int()
                        .ok_or_else(|| RuntimeError::Validation("比较需要 Int".into()))?,
                    r.as_int()
                        .ok_or_else(|| RuntimeError::Validation("比较需要 Int".into()))?,
                );
                Value::Bool(match op {
                    Lt => a < b,
                    Le => a <= b,
                    Gt => a > b,
                    Ge => a >= b,
                    _ => unreachable!(),
                })
            }
            Add => self.eval_add(l, r)?,
            Sub => {
                let (a, b) = int2(&l, &r, "-")?;
                Value::Int(a - b)
            }
            Mul => {
                let (a, b) = int2(&l, &r, "*")?;
                Value::Int(a * b)
            }
        };
        Ok(res)
    }

    /// `+`：Int 加法 / Text 拼接 / List 追加。
    fn eval_add(&self, l: Value, r: Value) -> Result<Value, RuntimeError> {
        match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::Text(a), Value::Text(b)) => Ok(Value::Text(a + &b)),
            (Value::List(mut a), Value::List(b)) => {
                a.extend(b);
                Ok(Value::List(a))
            }
            (l, r) => Err(RuntimeError::Validation(format!("`+` 不支持 {l} 与 {r}"))),
        }
    }
}

/// 取两个 Int 操作数。
fn int2(l: &Value, r: &Value, op: &str) -> Result<(i64, i64), RuntimeError> {
    match (l.as_int(), r.as_int()) {
        (Some(a), Some(b)) => Ok((a, b)),
        _ => Err(RuntimeError::Validation(format!("`{op}` 需要 Int 操作数"))),
    }
}

/// 值相等（结构相等）。
fn values_equal(a: &Value, b: &Value) -> bool {
    a == b
}

/// 尝试用 pattern 匹配值；成功返回绑定列表。
fn match_pattern(pattern: &Pattern, subject: &Value) -> Option<Vec<(String, Value)>> {
    match (pattern, subject) {
        (Pattern::Bool { value, .. }, Value::Bool(b)) if value == b => Some(vec![]),
        (Pattern::Null { .. }, Value::Null) => Some(vec![]),
        (Pattern::State { value, .. }, Value::State { value: v, .. }) if value.text == *v => {
            Some(vec![])
        }
        // 类型 pattern `Int x`：值的运行时类型 tag 与 pattern 类型名一致则绑定整个值。
        (Pattern::Type { ty, binding, .. }, _) if value_matches_type_name(subject, &ty.text) => {
            Some(vec![(binding.text.clone(), subject.clone())])
        }
        // error variant pattern `V { f }`：匹配同名 ErrorValue，按字段名绑定。
        (
            Pattern::Variant {
                variant, fields, ..
            },
            Value::ErrorValue {
                variant: v,
                fields: fvals,
            },
        ) if variant.text == *v => {
            let binds = fields
                .iter()
                .map(|f| {
                    let val = fvals.get(&f.text).cloned().unwrap_or(Value::Unit);
                    (f.text.clone(), val)
                })
                .collect();
            Some(binds)
        }
        _ => None,
    }
}

/// 值的运行时类型 tag 是否匹配类型 pattern 的类型名（标量 / entity / state）。
fn value_matches_type_name(v: &Value, name: &str) -> bool {
    match v {
        Value::Unit => name == "Unit",
        Value::Bool(_) => name == "Bool",
        Value::Int(_) => name == "Int",
        Value::Text(_) => name == "Text",
        Value::Null => name == "Null",
        Value::List(_) => false,
        Value::Entity { name: n, .. } => name == n,
        Value::State { state, .. } => name == state,
        Value::ErrorValue { .. } => false,
    }
}
