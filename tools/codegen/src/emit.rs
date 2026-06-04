//! WASM 模块 emit（W2 标量/控制流/聚合/Text；W4 effect host import：Console + registry op）。
//!
//! 见 docs/wasm_codegen.md §四 / §五 / §六。把 [`CodegenInput`] 投影为 WASM 模块字节。
//!
//! **覆盖**：值 `Unit` / `Bool` / `Int` / `Null` / `Text` / `ErrorValue` / `Entity` / `State`；
//! 全部纯逻辑语句/表达式（见 W2a–W2d）+ **effect**：`print`（Console.Write）与注册表里的
//! `Family.Op` 经 host import 委派（命名空间 `sophia_host`，ValueWire 字节 ABI），capability 边界在编译期
//! 语义层兑现、真实 vs mock host 由实例化方提供。**诚实 `NotYetImplemented`**：`to_text` / `List` /
//! `list.append`（无 v1 演示需求，YAGNI）。
//!
//! **首要不变量**：所有指令生成与解释器 `eval` / `exec_stmt` 一一对应（值 ABI / 函数 ABI / effect ABI
//! 见 docs/wasm_codegen.md），差测试兜底等价。

use crate::abi::{self, outcome, tag};
use crate::contract::CodegenInput;
use crate::error::{CodegenError, CodegenResult};
use sophia_hir::{LibraryRegistry, OpContract, Scalar, TypeDesc};
use sophia_syntax::{Ast, BinOp, Block, Callable, ElseBranch, Expr, ExprId, Item, Pattern, Stmt};
use std::collections::{BTreeMap, BTreeSet};
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, DataSection, ExportKind, ExportSection, Function,
    FunctionSection, GlobalSection, GlobalType, Instruction, MemArg, MemorySection, MemoryType,
    Module, TypeSection, ValType,
};

/// 常量字符串区起始（variant / 字段名 data section）。保留 0 作哨兵。
const DATA_BASE: i32 = 8;
/// 线性内存初始页数（每页 64KiB；差测试足够）。
const MEM_PAGES: u64 = 16;

/// 固定 host import：Console 是语言内置，read_copy 是 ValueWire 返回字节拷贝通道。
const CONSOLE_WRITE_IMPORT: u32 = 0;
const READ_COPY_IMPORT: u32 = 1;
const OP_IMPORT_BASE: u32 = 2;

/// ValueWire 标签（host 边界稳定字节协议）。
mod wire {
    pub const UNIT: u8 = 0;
    pub const BOOL: u8 = 1;
    pub const INT: u8 = 2;
    pub const TEXT: u8 = 3;
}

/// prelude 辅助函数相对索引。实际函数索引 = `import_count + helper::*`。
#[allow(dead_code)]
mod helper {
    pub const ALLOC: u32 = 0;
    pub const MAKE_INT: u32 = 1;
    pub const MAKE_BOOL: u32 = 2;
    pub const MAKE_NULL: u32 = 3;
    pub const MAKE_UNIT: u32 = 4;
    pub const GET_INT: u32 = 5;
    pub const GET_BOOL: u32 = 6;
    pub const VALUE_TAG: u32 = 7;
    pub const VALUE_EQ: u32 = 8;
    pub const WRAP_RETURNED: u32 = 9;
    pub const WRAP_RAISED: u32 = 10;
    pub const OUTCOME_KIND: u32 = 11;
    pub const OUTCOME_VALUE: u32 = 12;
    pub const RESET: u32 = 13;
    pub const STR_EQ: u32 = 14;
    pub const REC_FIELD: u32 = 15;
    pub const REC_NAME_EQ: u32 = 16;
    pub const MAKE_STATE: u32 = 17;
    pub const STATE_NAME_EQ: u32 = 18;
    pub const STATE_VALUE_EQ: u32 = 19;
    pub const MAKE_TEXT: u32 = 20;
    pub const TEXT_LENGTH: u32 = 21;
    pub const TEXT_CONCAT: u32 = 22;
    pub const GET_TEXT_PTR: u32 = 23;
    pub const GET_TEXT_LEN: u32 = 24;
}
/// prelude 函数个数。
const PRELUDE_COUNT: u32 = 25;

fn helper_index(import_count: u32, relative: u32) -> u32 {
    import_count + relative
}

#[derive(Debug, Clone)]
struct HostImport {
    contract: OpContract,
    index: u32,
}

#[derive(Debug, Clone)]
struct HostImports {
    ops: BTreeMap<(String, String), HostImport>,
}

impl HostImports {
    fn derive(input: &CodegenInput<'_>) -> CodegenResult<Self> {
        let mut used = BTreeSet::new();
        for ast in input.asts() {
            collect_effect_calls_ast(ast, input.registry(), &mut used);
        }
        let mut ops = BTreeMap::new();
        for (i, (family, op)) in used.into_iter().enumerate() {
            let contract = input
                .registry()
                .op(&family, &op)
                .ok_or_else(|| CodegenError::InvalidInput(format!("未知库 op `{family}.{op}`")))?
                .clone();
            validate_wire_contract(&contract)?;
            ops.insert(
                (family, op),
                HostImport {
                    contract,
                    index: OP_IMPORT_BASE + i as u32,
                },
            );
        }
        Ok(HostImports { ops })
    }

    fn import_count(&self) -> u32 {
        OP_IMPORT_BASE + self.ops.len() as u32
    }

    fn op_index(&self, family: &str, op: &str) -> Option<u32> {
        self.ops
            .get(&(family.to_string(), op.to_string()))
            .map(|i| i.index)
    }
}

fn validate_wire_contract(contract: &OpContract) -> CodegenResult<()> {
    for param in &contract.params {
        wire_scalar(param)?;
    }
    wire_scalar(&contract.returns)?;
    Ok(())
}

fn wire_scalar(desc: &TypeDesc) -> CodegenResult<Scalar> {
    match desc {
        TypeDesc::Scalar(s) => Ok(*s),
        TypeDesc::Intent { inner, .. } => Ok(*inner),
    }
}

fn wire_tag(scalar: Scalar) -> u8 {
    match scalar {
        Scalar::Unit => wire::UNIT,
        Scalar::Bool => wire::BOOL,
        Scalar::Int => wire::INT,
        Scalar::Text => wire::TEXT,
    }
}

const I32_MEM: u32 = 2; // align = 2^2 = 4
const I64_MEM: u32 = 3; // align = 2^3 = 8

fn mem(offset: u64, align: u32) -> MemArg {
    MemArg {
        offset,
        align,
        memory_index: 0,
    }
}

/// emit 一个程序为 WASM 模块字节。
pub fn emit(input: &CodegenInput<'_>) -> CodegenResult<Vec<u8>> {
    let model = input.model();
    let host_imports = HostImports::derive(input)?;
    let import_count = host_imports.import_count();

    // callable 名（按名字典序，与 model.callables BTreeMap / ExecGraph 同序，输出确定）。
    let callable_names: Vec<&String> = model.callables.keys().collect();

    // ---- 常量字符串区：预先把所有 variant / entity / state 名 + 字段名 intern 进 data section ----
    // （记录 / State 值引用它们的 ptr/len；先定 offset，code 才能引用常量。按名字典序，确定。）
    let mut interner = StrInterner::new(DATA_BASE);
    for (vname, vdecl) in &model.variants {
        interner.intern(vname);
        for (fname, _) in &vdecl.fields {
            interner.intern(fname);
        }
    }
    for (ename, edecl) in &model.entities {
        interner.intern(ename);
        for (fname, _) in &edecl.fields {
            interner.intern(fname);
        }
    }
    for (sname, sdecl) in &model.states {
        interner.intern(sname);
        for v in &sdecl.values {
            interner.intern(v);
        }
    }
    // 字符串字面量（body 内 `Expr::Str`）也进常量区——Text 值字面量引用其 ptr/len。
    for ast in input.asts() {
        for item in &ast.items {
            if let Item::Action(c) | Item::Transition(c) = item {
                if let Some(body) = &c.body {
                    intern_block_strings(body, ast, &mut interner);
                }
            }
        }
    }
    let bump_base = interner.bump_base();

    // ---- 类型段：import 类型 + prelude（25）+ 每 callable（N）。type_idx == func_idx ----
    let mut types = TypeSection::new();
    push_import_types(&mut types, &host_imports);
    push_prelude_types(&mut types);
    for name in &callable_names {
        let decl = &model.callables[*name];
        let params = vec![ValType::I32; decl.inputs.len()];
        types.ty().function(params, [ValType::I32]);
    }

    // ---- 导入段：Console + read_copy + 实际使用到的 registry effect op ----
    let mut imports_sec = wasm_encoder::ImportSection::new();
    imports_sec.import(
        "sophia_host",
        "console_write",
        wasm_encoder::EntityType::Function(CONSOLE_WRITE_IMPORT),
    );
    imports_sec.import(
        "sophia_host",
        "read_copy",
        wasm_encoder::EntityType::Function(READ_COPY_IMPORT),
    );
    for import in host_imports.ops.values() {
        imports_sec.import(
            &format!("sophia_lib:{}", import.contract.lib),
            &import.contract.host_fn,
            wasm_encoder::EntityType::Function(import.index),
        );
    }

    // ---- 函数段：定义函数（prelude + callables）引用各自 type 索引（= 自身 func 索引）----
    let mut functions = FunctionSection::new();
    let defined = PRELUDE_COUNT + callable_names.len() as u32;
    for i in 0..defined {
        functions.function(import_count + i);
    }

    // ---- 内存段 ----
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: MEM_PAGES,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    // ---- 全局段：可变 bump 指针（起始在常量区之后）----
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(bump_base),
    );

    // ---- 代码段 ----
    let mut code = CodeSection::new();
    emit_prelude(&mut code, bump_base, import_count);

    // callable 名 → 函数索引（跨调用解析）。imports 在前，prelude 居中，callables 在后。
    let func_index: BTreeMap<String, u32> = callable_names
        .iter()
        .enumerate()
        .map(|(i, n)| ((*n).clone(), import_count + PRELUDE_COUNT + i as u32))
        .collect();

    for name in &callable_names {
        let (ast, callable) = find_callable(input.asts(), name)
            .ok_or_else(|| CodegenError::InvalidInput(format!("callable `{name}` 无 AST")))?;
        let ctx = EmitContext {
            model,
            lib_index: input.lib_index(),
            host_imports: &host_imports,
            import_count,
            func_index: &func_index,
            interner: &interner,
        };
        let func = emit_callable(ast, callable, &ctx)?;
        code.function(&func);
    }

    // ---- 导出段：内存 + 测试 / host 可见的 helper + 全部 callable ----
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export(
        "sophia_alloc",
        ExportKind::Func,
        helper_index(import_count, helper::ALLOC),
    );
    exports.export(
        "sophia_make_int",
        ExportKind::Func,
        helper_index(import_count, helper::MAKE_INT),
    );
    exports.export(
        "sophia_make_bool",
        ExportKind::Func,
        helper_index(import_count, helper::MAKE_BOOL),
    );
    exports.export(
        "sophia_make_null",
        ExportKind::Func,
        helper_index(import_count, helper::MAKE_NULL),
    );
    exports.export(
        "sophia_make_unit",
        ExportKind::Func,
        helper_index(import_count, helper::MAKE_UNIT),
    );
    exports.export(
        "sophia_value_tag",
        ExportKind::Func,
        helper_index(import_count, helper::VALUE_TAG),
    );
    exports.export(
        "sophia_get_int",
        ExportKind::Func,
        helper_index(import_count, helper::GET_INT),
    );
    exports.export(
        "sophia_get_bool",
        ExportKind::Func,
        helper_index(import_count, helper::GET_BOOL),
    );
    exports.export(
        "sophia_outcome_kind",
        ExportKind::Func,
        helper_index(import_count, helper::OUTCOME_KIND),
    );
    exports.export(
        "sophia_outcome_value",
        ExportKind::Func,
        helper_index(import_count, helper::OUTCOME_VALUE),
    );
    exports.export(
        "sophia_rec_field",
        ExportKind::Func,
        helper_index(import_count, helper::REC_FIELD),
    );
    exports.export(
        "sophia_make_state",
        ExportKind::Func,
        helper_index(import_count, helper::MAKE_STATE),
    );
    exports.export(
        "sophia_make_text",
        ExportKind::Func,
        helper_index(import_count, helper::MAKE_TEXT),
    );
    exports.export(
        "sophia_get_text_ptr",
        ExportKind::Func,
        helper_index(import_count, helper::GET_TEXT_PTR),
    );
    exports.export(
        "sophia_get_text_len",
        ExportKind::Func,
        helper_index(import_count, helper::GET_TEXT_LEN),
    );
    exports.export(
        "sophia_reset",
        ExportKind::Func,
        helper_index(import_count, helper::RESET),
    );
    for name in &callable_names {
        let decl = &model.callables[*name];
        let prefix = match decl.kind {
            sophia_syntax::CallableKind::Action => "action_",
            sophia_syntax::CallableKind::Transition => "transition_",
        };
        exports.export(
            &format!("{prefix}{name}"),
            ExportKind::Func,
            func_index[*name],
        );
    }

    // ---- 数据段：常量字符串区 ----
    let mut data = DataSection::new();
    if !interner.bytes.is_empty() {
        data.active(
            0,
            &ConstExpr::i32_const(DATA_BASE),
            interner.bytes.iter().copied(),
        );
    }

    // ---- 组装模块（段顺序固定，保证字节确定）----
    let mut module = Module::new();
    module.section(&types);
    module.section(&imports_sec);
    module.section(&functions);
    module.section(&memories);
    module.section(&globals);
    module.section(&exports);
    module.section(&code);
    module.section(&data);
    Ok(module.finish())
}

/// import 段的函数类型。
fn push_import_types(types: &mut TypeSection, host_imports: &HostImports) {
    // console_write(ptr, len)
    types.ty().function([ValType::I32, ValType::I32], []);
    // read_copy(dst_ptr)
    types.ty().function([ValType::I32], []);
    // registry op：ValueWire args bytes -> host stash result byte length。
    for _ in host_imports.ops.values() {
        types
            .ty()
            .function([ValType::I32, ValType::I32], [ValType::I32]);
    }
}

/// 常量字符串 intern 器：把字符串顺序写入 data 缓冲，记录每串的 (ptr, len)。
///
/// data 区从 `DATA_BASE` 起；bump 堆在常量区之后（8 对齐）。同串去重（同 ptr/len）。
struct StrInterner {
    base: i32,
    bytes: Vec<u8>,
    offsets: BTreeMap<String, (i32, i32)>,
}

impl StrInterner {
    fn new(base: i32) -> Self {
        StrInterner {
            base,
            bytes: Vec::new(),
            offsets: BTreeMap::new(),
        }
    }

    /// intern 一个字符串，返回 (ptr, len)。
    fn intern(&mut self, s: &str) -> (i32, i32) {
        if let Some(&pl) = self.offsets.get(s) {
            return pl;
        }
        let ptr = self.base + self.bytes.len() as i32;
        let len = s.len() as i32;
        self.bytes.extend_from_slice(s.as_bytes());
        self.offsets.insert(s.to_string(), (ptr, len));
        (ptr, len)
    }

    /// 已 intern 串的 (ptr, len)（必须先 intern）。
    fn lookup(&self, s: &str) -> (i32, i32) {
        self.offsets[s]
    }

    /// bump 堆起始：常量区之后向上对齐到 8。
    fn bump_base(&self) -> i32 {
        let end = self.base + self.bytes.len() as i32;
        (end + (abi::ALLOC_ALIGN - 1)) & !(abi::ALLOC_ALIGN - 1)
    }
}

/// 把一个 block 内的全部字符串字面量（`Expr::Str`）intern 进常量区（Text 值字面量需要其 ptr/len）。
fn intern_block_strings(block: &Block, ast: &Ast, interner: &mut StrInterner) {
    for stmt in &block.stmts {
        intern_stmt_strings(stmt, ast, interner);
    }
}

fn intern_stmt_strings(stmt: &Stmt, ast: &Ast, interner: &mut StrInterner) {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::Set { value, .. }
        | Stmt::Return { value, .. }
        | Stmt::Raise { value, .. }
        | Stmt::Print { value, .. }
        | Stmt::Expr { value, .. } => intern_expr_strings(*value, ast, interner),
        Stmt::If {
            condition,
            consequence,
            alternative,
            ..
        } => {
            intern_expr_strings(*condition, ast, interner);
            intern_block_strings(consequence, ast, interner);
            match alternative {
                Some(ElseBranch::Block(b)) => intern_block_strings(b, ast, interner),
                Some(ElseBranch::If(s)) => intern_stmt_strings(s, ast, interner),
                None => {}
            }
        }
        Stmt::Match { subject, arms, .. } => {
            intern_expr_strings(*subject, ast, interner);
            for arm in arms {
                intern_block_strings(&arm.body, ast, interner);
            }
        }
        Stmt::Repeat { count, body, .. } => {
            intern_expr_strings(*count, ast, interner);
            intern_block_strings(body, ast, interner);
        }
    }
}

fn intern_expr_strings(id: ExprId, ast: &Ast, interner: &mut StrInterner) {
    match ast.expr(id) {
        Expr::Str(s) => {
            interner.intern(&s.value);
        }
        Expr::List { items, .. } => {
            for &it in items {
                intern_expr_strings(it, ast, interner);
            }
        }
        Expr::Field { base, .. } => intern_expr_strings(*base, ast, interner),
        Expr::MethodCall { base, args, .. } => {
            intern_expr_strings(*base, ast, interner);
            for &a in args {
                intern_expr_strings(a, ast, interner);
            }
        }
        Expr::Call { args, .. } => {
            for &a in args {
                intern_expr_strings(a, ast, interner);
            }
        }
        Expr::Construct { fields, .. } => {
            for fi in fields {
                intern_expr_strings(fi.value, ast, interner);
            }
        }
        Expr::Not { operand, .. } | Expr::Neg { operand, .. } => {
            intern_expr_strings(*operand, ast, interner)
        }
        Expr::Binary { left, right, .. } => {
            intern_expr_strings(*left, ast, interner);
            intern_expr_strings(*right, ast, interner);
        }
        Expr::Int { .. } | Expr::Bool { .. } | Expr::Null { .. } | Expr::Ident(_) => {}
    }
}

fn collect_effect_calls_ast(
    ast: &Ast,
    registry: &LibraryRegistry,
    out: &mut BTreeSet<(String, String)>,
) {
    for item in &ast.items {
        if let Item::Action(c) | Item::Transition(c) = item {
            if let Some(body) = &c.body {
                collect_effect_calls_block(body, ast, registry, out);
            }
        }
    }
}

fn collect_effect_calls_block(
    block: &Block,
    ast: &Ast,
    registry: &LibraryRegistry,
    out: &mut BTreeSet<(String, String)>,
) {
    for stmt in &block.stmts {
        collect_effect_calls_stmt(stmt, ast, registry, out);
    }
}

fn collect_effect_calls_stmt(
    stmt: &Stmt,
    ast: &Ast,
    registry: &LibraryRegistry,
    out: &mut BTreeSet<(String, String)>,
) {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::Set { value, .. }
        | Stmt::Return { value, .. }
        | Stmt::Raise { value, .. }
        | Stmt::Print { value, .. }
        | Stmt::Expr { value, .. } => collect_effect_calls_expr(*value, ast, registry, out),
        Stmt::If {
            condition,
            consequence,
            alternative,
            ..
        } => {
            collect_effect_calls_expr(*condition, ast, registry, out);
            collect_effect_calls_block(consequence, ast, registry, out);
            match alternative {
                Some(ElseBranch::Block(b)) => collect_effect_calls_block(b, ast, registry, out),
                Some(ElseBranch::If(s)) => collect_effect_calls_stmt(s, ast, registry, out),
                None => {}
            }
        }
        Stmt::Match { subject, arms, .. } => {
            collect_effect_calls_expr(*subject, ast, registry, out);
            for arm in arms {
                collect_effect_calls_block(&arm.body, ast, registry, out);
            }
        }
        Stmt::Repeat { count, body, .. } => {
            collect_effect_calls_expr(*count, ast, registry, out);
            collect_effect_calls_block(body, ast, registry, out);
        }
    }
}

fn collect_effect_calls_expr(
    id: ExprId,
    ast: &Ast,
    registry: &LibraryRegistry,
    out: &mut BTreeSet<(String, String)>,
) {
    match ast.expr(id) {
        Expr::MethodCall {
            base, method, args, ..
        } => {
            if let Expr::Ident(root) = ast.expr(*base) {
                if registry.op(&root.text, &method.text).is_some() {
                    out.insert((root.text.clone(), method.text.clone()));
                }
            }
            collect_effect_calls_expr(*base, ast, registry, out);
            for &a in args {
                collect_effect_calls_expr(a, ast, registry, out);
            }
        }
        Expr::List { items, .. } => {
            for &it in items {
                collect_effect_calls_expr(it, ast, registry, out);
            }
        }
        Expr::Field { base, .. } => collect_effect_calls_expr(*base, ast, registry, out),
        Expr::Call { args, .. } => {
            for &a in args {
                collect_effect_calls_expr(a, ast, registry, out);
            }
        }
        Expr::Construct { fields, .. } => {
            for fi in fields {
                collect_effect_calls_expr(fi.value, ast, registry, out);
            }
        }
        Expr::Not { operand, .. } | Expr::Neg { operand, .. } => {
            collect_effect_calls_expr(*operand, ast, registry, out)
        }
        Expr::Binary { left, right, .. } => {
            collect_effect_calls_expr(*left, ast, registry, out);
            collect_effect_calls_expr(*right, ast, registry, out);
        }
        Expr::Int { .. }
        | Expr::Bool { .. }
        | Expr::Null { .. }
        | Expr::Ident(_)
        | Expr::Str(_) => {}
    }
}

fn find_callable<'a>(asts: &'a [&'a Ast], name: &str) -> Option<(&'a Ast, &'a Callable)> {
    asts.iter().find_map(|&ast| {
        ast.items.iter().find_map(|it| match it {
            Item::Action(c) | Item::Transition(c) if c.name.text == name => Some((ast, c)),
            _ => None,
        })
    })
}

// ============ prelude（值 ABI / 函数 ABI 的运行时 helper，生成进 module 自身）============

fn push_prelude_types(types: &mut TypeSection) {
    types.ty().function([ValType::I32], [ValType::I32]); // alloc
    types.ty().function([ValType::I64], [ValType::I32]); // make_int
    types.ty().function([ValType::I32], [ValType::I32]); // make_bool
    types.ty().function([], [ValType::I32]); // make_null
    types.ty().function([], [ValType::I32]); // make_unit
    types.ty().function([ValType::I32], [ValType::I64]); // get_int
    types.ty().function([ValType::I32], [ValType::I32]); // get_bool
    types.ty().function([ValType::I32], [ValType::I32]); // value_tag
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]); // value_eq
    types.ty().function([ValType::I32], [ValType::I32]); // wrap_returned
    types.ty().function([ValType::I32], [ValType::I32]); // wrap_raised
    types.ty().function([ValType::I32], [ValType::I32]); // outcome_kind
    types.ty().function([ValType::I32], [ValType::I32]); // outcome_value
    types.ty().function([], []); // reset
                                 // str_eq(ptr_a, len_a, ptr_b, len_b) -> i32
    types.ty().function(
        [ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        [ValType::I32],
    );
    // rec_field(rec, key_ptr, key_len) -> val_handle（未命中返回 0）
    types
        .ty()
        .function([ValType::I32, ValType::I32, ValType::I32], [ValType::I32]);
    // rec_name_eq(rec, name_ptr, name_len) -> i32（记录名是否等）
    types
        .ty()
        .function([ValType::I32, ValType::I32, ValType::I32], [ValType::I32]);
    // make_state(state_ptr, state_len, value_ptr, value_len) -> handle
    types.ty().function(
        [ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        [ValType::I32],
    );
    // state_name_eq(state, name_ptr, name_len) -> i32（state 名是否等）
    types
        .ty()
        .function([ValType::I32, ValType::I32, ValType::I32], [ValType::I32]);
    // state_value_eq(state, value_ptr, value_len) -> i32（value 名是否等）
    types
        .ty()
        .function([ValType::I32, ValType::I32, ValType::I32], [ValType::I32]);
    // make_text(bytes_ptr, byte_len) -> handle
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    // text_length(h) -> handle（Int 值，Unicode 标量计数）
    types.ty().function([ValType::I32], [ValType::I32]);
    // text_concat(a, b) -> handle（Text 值，bump 分配新串）
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    // get_text_ptr(h) -> i32 / get_text_len(h) -> i32
    types.ty().function([ValType::I32], [ValType::I32]);
    types.ty().function([ValType::I32], [ValType::I32]);
}

fn emit_prelude(code: &mut CodeSection, bump_base: i32, import_count: u32) {
    code.function(&f_alloc());
    code.function(&f_make_int(import_count));
    code.function(&f_make_bool(import_count));
    code.function(&f_make_null_or_unit(tag::NULL, import_count));
    code.function(&f_make_null_or_unit(tag::UNIT, import_count));
    code.function(&f_get_int());
    code.function(&f_get_bool());
    code.function(&f_value_tag());
    code.function(&f_value_eq(import_count));
    code.function(&f_wrap_outcome(outcome::RETURNED, import_count));
    code.function(&f_wrap_outcome(outcome::RAISED, import_count));
    code.function(&f_outcome_field(abi::OFF_TAG)); // outcome_kind (kind @0)
    code.function(&f_outcome_field(abi::OFF_OUTCOME_VALUE)); // outcome_value (value @4)
    code.function(&f_reset(bump_base));
    code.function(&f_str_eq());
    code.function(&f_rec_field(import_count));
    code.function(&f_rec_name_eq(import_count));
    code.function(&f_make_state(import_count));
    code.function(&f_state_field_eq(
        abi::OFF_STATE_NAME_PTR,
        abi::OFF_STATE_NAME_LEN,
        import_count,
    ));
    code.function(&f_state_field_eq(
        abi::OFF_STATE_VALUE_PTR,
        abi::OFF_STATE_VALUE_LEN,
        import_count,
    ));
    code.function(&f_make_text(import_count));
    code.function(&f_text_length(import_count));
    code.function(&f_text_concat(import_count));
    code.function(&f_text_field(abi::OFF_TEXT_PTR));
    code.function(&f_text_field(abi::OFF_TEXT_LEN));
}

/// `alloc(size) -> ptr`：bump 分配，bump 指针向上对齐 8。param0=size:i32。
fn f_alloc() -> Function {
    let mut f = Function::new([]);
    f.instruction(&Instruction::GlobalGet(0)); // old (return value)
    f.instruction(&Instruction::GlobalGet(0));
    f.instruction(&Instruction::LocalGet(0)); // size
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Const(abi::ALLOC_ALIGN - 1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Const(-abi::ALLOC_ALIGN)); // ~(align-1)
    f.instruction(&Instruction::I32And);
    f.instruction(&Instruction::GlobalSet(0));
    f.instruction(&Instruction::End);
    f
}

/// `make_int(v:i64) -> handle`。param0=v:i64, local1=h:i32。
fn f_make_int(import_count: u32) -> Function {
    let mut f = Function::new([(1, ValType::I32)]);
    f.instruction(&Instruction::I32Const(abi::SIZE_INT));
    f.instruction(&Instruction::Call(helper_index(
        import_count,
        helper::ALLOC,
    )));
    f.instruction(&Instruction::LocalSet(1));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Const(tag::INT));
    f.instruction(&Instruction::I32Store(mem(abi::OFF_TAG, I32_MEM)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I64Store(mem(abi::OFF_INT_PAYLOAD, I64_MEM)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::End);
    f
}

/// `make_bool(b:i32) -> handle`。param0=b:i32, local1=h:i32。
fn f_make_bool(import_count: u32) -> Function {
    let mut f = Function::new([(1, ValType::I32)]);
    f.instruction(&Instruction::I32Const(abi::SIZE_BOOL));
    f.instruction(&Instruction::Call(helper_index(
        import_count,
        helper::ALLOC,
    )));
    f.instruction(&Instruction::LocalSet(1));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Const(tag::BOOL));
    f.instruction(&Instruction::I32Store(mem(abi::OFF_TAG, I32_MEM)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Store(mem(abi::OFF_BOOL_PAYLOAD, I32_MEM)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::End);
    f
}

/// `make_null() / make_unit() -> handle`（仅 tag）。local0=h:i32。
fn f_make_null_or_unit(value_tag: i32, import_count: u32) -> Function {
    let mut f = Function::new([(1, ValType::I32)]);
    f.instruction(&Instruction::I32Const(abi::SIZE_TAGONLY));
    f.instruction(&Instruction::Call(helper_index(
        import_count,
        helper::ALLOC,
    )));
    f.instruction(&Instruction::LocalSet(0));
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Const(value_tag));
    f.instruction(&Instruction::I32Store(mem(abi::OFF_TAG, I32_MEM)));
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::End);
    f
}

/// `get_int(h) -> i64`。
fn f_get_int() -> Function {
    let mut f = Function::new([]);
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I64Load(mem(abi::OFF_INT_PAYLOAD, I64_MEM)));
    f.instruction(&Instruction::End);
    f
}

/// `get_bool(h) -> i32`。
fn f_get_bool() -> Function {
    let mut f = Function::new([]);
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_BOOL_PAYLOAD, I32_MEM)));
    f.instruction(&Instruction::End);
    f
}

/// `value_tag(h) -> i32`。
fn f_value_tag() -> Function {
    let mut f = Function::new([]);
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_TAG, I32_MEM)));
    f.instruction(&Instruction::End);
    f
}

/// `value_eq(a, b) -> i32`（结构相等：Int / Bool / Null / Unit / Text）。local2=t:i32。
fn f_value_eq(import_count: u32) -> Function {
    let mut f = Function::new([(1, ValType::I32)]);
    // tag(a) != tag(b) → 0
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_TAG, I32_MEM)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_TAG, I32_MEM)));
    f.instruction(&Instruction::I32Ne);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);
    // t = tag(a)
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_TAG, I32_MEM)));
    f.instruction(&Instruction::LocalTee(2));
    f.instruction(&Instruction::I32Const(tag::INT));
    f.instruction(&Instruction::I32Eq);
    f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    // Int：比较 i64 payload
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I64Load(mem(abi::OFF_INT_PAYLOAD, I64_MEM)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I64Load(mem(abi::OFF_INT_PAYLOAD, I64_MEM)));
    f.instruction(&Instruction::I64Eq);
    f.instruction(&Instruction::Else);
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::I32Const(tag::BOOL));
    f.instruction(&Instruction::I32Eq);
    f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    // Bool：比较 i32 payload
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_BOOL_PAYLOAD, I32_MEM)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_BOOL_PAYLOAD, I32_MEM)));
    f.instruction(&Instruction::I32Eq);
    f.instruction(&Instruction::Else);
    // Null / Unit / Text 等 tag 相等的情形：Text 比字节，其余 tag 相等即相等。
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::I32Const(tag::TEXT));
    f.instruction(&Instruction::I32Eq);
    f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    // Text：str_eq(a.ptr, a.len, b.ptr, b.len)
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_TEXT_PTR, I32_MEM)));
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_TEXT_LEN, I32_MEM)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_TEXT_PTR, I32_MEM)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_TEXT_LEN, I32_MEM)));
    f.instruction(&Instruction::Call(helper_index(
        import_count,
        helper::STR_EQ,
    )));
    f.instruction(&Instruction::Else);
    // Null / Unit：tag 相等即相等
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::End);
    f.instruction(&Instruction::End);
    f.instruction(&Instruction::End);
    f.instruction(&Instruction::End);
    f
}

/// `wrap_returned(v) / wrap_raised(v) -> outcome`。param0=v:i32, local1=oc:i32。
fn f_wrap_outcome(kind: i32, import_count: u32) -> Function {
    let mut f = Function::new([(1, ValType::I32)]);
    f.instruction(&Instruction::I32Const(abi::SIZE_OUTCOME));
    f.instruction(&Instruction::Call(helper_index(
        import_count,
        helper::ALLOC,
    )));
    f.instruction(&Instruction::LocalSet(1));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Const(kind));
    f.instruction(&Instruction::I32Store(mem(abi::OFF_TAG, I32_MEM)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Store(mem(abi::OFF_OUTCOME_VALUE, I32_MEM)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::End);
    f
}

/// `outcome_kind(oc) / outcome_value(oc) -> i32`（按字段偏移读）。
fn f_outcome_field(offset: u64) -> Function {
    let mut f = Function::new([]);
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(offset, I32_MEM)));
    f.instruction(&Instruction::End);
    f
}

/// `reset()`：bump 指针归位到常量区之后（差测试每次执行前调用）。
fn f_reset(bump_base: i32) -> Function {
    let mut f = Function::new([]);
    f.instruction(&Instruction::I32Const(bump_base));
    f.instruction(&Instruction::GlobalSet(0));
    f.instruction(&Instruction::End);
    f
}

/// `str_eq(ptr_a, len_a, ptr_b, len_b) -> i32`：字节序列相等。
/// params 0..3；local4=i:i32。
fn f_str_eq() -> Function {
    let mut f = Function::new([(1, ValType::I32)]);
    // len_a != len_b → 0
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32Ne);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);
    // i = 0; loop while i < len_a: if a[i] != b[i] return 0; i++
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalSet(4));
    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));
    // if i >= len_a: break (br 1 → 出 block)
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32GeU);
    f.instruction(&Instruction::BrIf(1));
    // a[ptr_a + i]
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Load8U(mem(0, 0)));
    // b[ptr_b + i]
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Load8U(mem(0, 0)));
    f.instruction(&Instruction::I32Ne);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);
    // i++
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(4));
    f.instruction(&Instruction::Br(0));
    f.instruction(&Instruction::End); // loop
    f.instruction(&Instruction::End); // block
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::End);
    f
}

/// `rec_name_eq(rec, name_ptr, name_len) -> i32`：记录名（variant 名）是否等于给定常量串。
fn f_rec_name_eq(import_count: u32) -> Function {
    let mut f = Function::new([]);
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_REC_NAME_PTR, I32_MEM)));
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_REC_NAME_LEN, I32_MEM)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::Call(helper_index(
        import_count,
        helper::STR_EQ,
    )));
    f.instruction(&Instruction::End);
    f
}

/// `rec_field(rec, key_ptr, key_len) -> val_handle`：按字段名查记录字段值，未命中返回 0。
/// params 0..2；local3=n:i32, local4=i:i32, local5=base:i32。
fn f_rec_field(import_count: u32) -> Function {
    let mut f = Function::new([(3, ValType::I32)]);
    // n = nfields
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_REC_NFIELDS, I32_MEM)));
    f.instruction(&Instruction::LocalSet(3));
    // i = 0
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalSet(4));
    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));
    // if i >= n: break
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32GeU);
    f.instruction(&Instruction::BrIf(1));
    // base = rec + REC_HEADER_SIZE + i * REC_FIELD_SIZE
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Const(abi::REC_HEADER_SIZE));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::I32Const(abi::REC_FIELD_SIZE));
    f.instruction(&Instruction::I32Mul);
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(5));
    // if str_eq(field.key_ptr, field.key_len, key_ptr, key_len): return field.val
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_FIELD_KEY_PTR, I32_MEM)));
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_FIELD_KEY_LEN, I32_MEM)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::Call(helper_index(
        import_count,
        helper::STR_EQ,
    )));
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_FIELD_VAL, I32_MEM)));
    f.instruction(&Instruction::Return);
    f.instruction(&Instruction::End);
    // i++
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(4));
    f.instruction(&Instruction::Br(0));
    f.instruction(&Instruction::End); // loop
    f.instruction(&Instruction::End); // block
    f.instruction(&Instruction::I32Const(0)); // 未命中
    f.instruction(&Instruction::End);
    f
}

/// `make_state(state_ptr, state_len, value_ptr, value_len) -> handle`。params 0..3, local4=h:i32。
fn f_make_state(import_count: u32) -> Function {
    let mut f = Function::new([(1, ValType::I32)]);
    f.instruction(&Instruction::I32Const(abi::SIZE_STATE));
    f.instruction(&Instruction::Call(helper_index(
        import_count,
        helper::ALLOC,
    )));
    f.instruction(&Instruction::LocalSet(4));
    // tag
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::I32Const(tag::STATE));
    f.instruction(&Instruction::I32Store(mem(abi::OFF_TAG, I32_MEM)));
    // state_ptr / state_len
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Store(mem(
        abi::OFF_STATE_NAME_PTR,
        I32_MEM,
    )));
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Store(mem(
        abi::OFF_STATE_NAME_LEN,
        I32_MEM,
    )));
    // value_ptr / value_len
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::I32Store(mem(
        abi::OFF_STATE_VALUE_PTR,
        I32_MEM,
    )));
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32Store(mem(
        abi::OFF_STATE_VALUE_LEN,
        I32_MEM,
    )));
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::End);
    f
}

/// `state_name_eq / state_value_eq(state, ptr, len) -> i32`：state / value 名等于给定常量串。
fn f_state_field_eq(off_ptr: u64, off_len: u64, import_count: u32) -> Function {
    let mut f = Function::new([]);
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(off_ptr, I32_MEM)));
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(off_len, I32_MEM)));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::Call(helper_index(
        import_count,
        helper::STR_EQ,
    )));
    f.instruction(&Instruction::End);
    f
}

/// `make_text(bytes_ptr, byte_len) -> handle`。params 0..1, local2=h:i32。
fn f_make_text(import_count: u32) -> Function {
    let mut f = Function::new([(1, ValType::I32)]);
    f.instruction(&Instruction::I32Const(abi::SIZE_TEXT));
    f.instruction(&Instruction::Call(helper_index(
        import_count,
        helper::ALLOC,
    )));
    f.instruction(&Instruction::LocalSet(2));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::I32Const(tag::TEXT));
    f.instruction(&Instruction::I32Store(mem(abi::OFF_TAG, I32_MEM)));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Store(mem(abi::OFF_TEXT_PTR, I32_MEM)));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Store(mem(abi::OFF_TEXT_LEN, I32_MEM)));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::End);
    f
}

/// `get_text_ptr / get_text_len(h) -> i32`。
fn f_text_field(offset: u64) -> Function {
    let mut f = Function::new([]);
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(offset, I32_MEM)));
    f.instruction(&Instruction::End);
    f
}

/// `text_length(h) -> Int handle`：UTF-8 字节序列的 **Unicode 标量计数**（与解释器
/// `chars().count()` 一致，非字节数）——统计非延续字节（top 2 bits != `10`）。
/// local1=ptr, local2=len, local3=i, local4=count, local5=byte。
fn f_text_length(import_count: u32) -> Function {
    let mut f = Function::new([(5, ValType::I32)]);
    // ptr = h.text_ptr; len = h.text_len
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_TEXT_PTR, I32_MEM)));
    f.instruction(&Instruction::LocalSet(1));
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_TEXT_LEN, I32_MEM)));
    f.instruction(&Instruction::LocalSet(2));
    // i = 0; count = 0
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalSet(3));
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalSet(4));
    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));
    // if i >= len: break
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::I32GeU);
    f.instruction(&Instruction::BrIf(1));
    // byte = mem[ptr + i] & 0xC0
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Load8U(mem(0, 0)));
    f.instruction(&Instruction::I32Const(0xC0));
    f.instruction(&Instruction::I32And);
    f.instruction(&Instruction::LocalSet(5));
    // if byte != 0x80 { count++ }（非延续字节 = 一个标量的起始字节）
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Const(0x80));
    f.instruction(&Instruction::I32Ne);
    f.instruction(&Instruction::If(BlockType::Empty));
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(4));
    f.instruction(&Instruction::End);
    // i++
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(3));
    f.instruction(&Instruction::Br(0));
    f.instruction(&Instruction::End); // loop
    f.instruction(&Instruction::End); // block
                                      // make_int(count as i64)
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::I64ExtendI32U);
    f.instruction(&Instruction::Call(helper_index(
        import_count,
        helper::MAKE_INT,
    )));
    f.instruction(&Instruction::End);
    f
}

/// `text_concat(a, b) -> Text handle`：在 bump 堆新建 `a.bytes ++ b.bytes`。
/// local2=la, local3=lb, local4=dst, local5=i。
fn f_text_concat(import_count: u32) -> Function {
    let mut f = Function::new([(4, ValType::I32)]);
    // la = a.len; lb = b.len
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_TEXT_LEN, I32_MEM)));
    f.instruction(&Instruction::LocalSet(2));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_TEXT_LEN, I32_MEM)));
    f.instruction(&Instruction::LocalSet(3));
    // dst = alloc(la + lb)
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::Call(helper_index(
        import_count,
        helper::ALLOC,
    )));
    f.instruction(&Instruction::LocalSet(4));
    // copy a.bytes → dst[0..la]：i=0; while i<la: dst[i]=a.ptr[i]; i++
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalSet(5));
    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::I32GeU);
    f.instruction(&Instruction::BrIf(1));
    // dst + i
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Add);
    // a.ptr + i 处的字节
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_TEXT_PTR, I32_MEM)));
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Load8U(mem(0, 0)));
    f.instruction(&Instruction::I32Store8(mem(0, 0)));
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(5));
    f.instruction(&Instruction::Br(0));
    f.instruction(&Instruction::End); // loop
    f.instruction(&Instruction::End); // block
                                      // copy b.bytes → dst[la..la+lb]：i=0; while i<lb: dst[la+i]=b.ptr[i]; i++
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::LocalSet(5));
    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32GeU);
    f.instruction(&Instruction::BrIf(1));
    // dst + la + i
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Add);
    // b.ptr + i 处的字节
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Load(mem(abi::OFF_TEXT_PTR, I32_MEM)));
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I32Load8U(mem(0, 0)));
    f.instruction(&Instruction::I32Store8(mem(0, 0)));
    f.instruction(&Instruction::LocalGet(5));
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::LocalSet(5));
    f.instruction(&Instruction::Br(0));
    f.instruction(&Instruction::End); // loop
    f.instruction(&Instruction::End); // block
                                      // make_text(dst, la + lb)
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::LocalGet(3));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::Call(helper_index(
        import_count,
        helper::MAKE_TEXT,
    )));
    f.instruction(&Instruction::End);
    f
}

// ============ callable body emit ============

/// 收集 callable body 内全部 `let` 绑定名（无 shadowing 由 HIR 保证，故名字唯一）。
fn collect_lets(block: &Block, out: &mut Vec<String>) {
    for stmt in &block.stmts {
        collect_lets_stmt(stmt, out);
    }
}

fn collect_lets_stmt(stmt: &Stmt, out: &mut Vec<String>) {
    match stmt {
        Stmt::Let { name, .. } => out.push(name.text.clone()),
        Stmt::If {
            consequence,
            alternative,
            ..
        } => {
            collect_lets(consequence, out);
            match alternative {
                Some(ElseBranch::Block(b)) => collect_lets(b, out),
                Some(ElseBranch::If(s)) => collect_lets_stmt(s, out),
                None => {}
            }
        }
        Stmt::Match { arms, .. } => {
            for arm in arms {
                // pattern 绑定（type binding / variant 字段）也需要 WASM 局部槽位。
                match &arm.pattern {
                    Pattern::Type { binding, .. } => out.push(binding.text.clone()),
                    Pattern::Variant { fields, .. } => {
                        for fb in fields {
                            out.push(fb.text.clone());
                        }
                    }
                    _ => {}
                }
                collect_lets(&arm.body, out);
            }
        }
        Stmt::Repeat { body, .. } => collect_lets(body, out),
        _ => {}
    }
}

fn max_effect_arg_count(block: &Block, ast: &Ast, host_imports: &HostImports) -> u32 {
    block
        .stmts
        .iter()
        .map(|stmt| max_effect_args_stmt(stmt, ast, host_imports))
        .max()
        .unwrap_or(0)
}

fn max_effect_args_stmt(stmt: &Stmt, ast: &Ast, host_imports: &HostImports) -> u32 {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::Set { value, .. }
        | Stmt::Return { value, .. }
        | Stmt::Raise { value, .. }
        | Stmt::Print { value, .. }
        | Stmt::Expr { value, .. } => max_effect_args_expr(*value, ast, host_imports),
        Stmt::If {
            condition,
            consequence,
            alternative,
            ..
        } => {
            let mut max = max_effect_args_expr(*condition, ast, host_imports)
                .max(max_effect_arg_count(consequence, ast, host_imports));
            if let Some(alt) = alternative {
                max = max.max(match alt {
                    ElseBranch::Block(b) => max_effect_arg_count(b, ast, host_imports),
                    ElseBranch::If(s) => max_effect_args_stmt(s, ast, host_imports),
                });
            }
            max
        }
        Stmt::Match { subject, arms, .. } => arms.iter().fold(
            max_effect_args_expr(*subject, ast, host_imports),
            |max, arm| max.max(max_effect_arg_count(&arm.body, ast, host_imports)),
        ),
        Stmt::Repeat { count, body, .. } => max_effect_args_expr(*count, ast, host_imports)
            .max(max_effect_arg_count(body, ast, host_imports)),
    }
}

fn max_effect_args_expr(id: ExprId, ast: &Ast, host_imports: &HostImports) -> u32 {
    match ast.expr(id) {
        Expr::MethodCall {
            base, method, args, ..
        } => {
            let own = match ast.expr(*base) {
                Expr::Ident(root) if host_imports.op_index(&root.text, &method.text).is_some() => {
                    args.len() as u32
                }
                _ => 0,
            };
            args.iter().fold(
                own.max(max_effect_args_expr(*base, ast, host_imports)),
                |max, &arg| max.max(max_effect_args_expr(arg, ast, host_imports)),
            )
        }
        Expr::List { items, .. } => items
            .iter()
            .map(|&it| max_effect_args_expr(it, ast, host_imports))
            .max()
            .unwrap_or(0),
        Expr::Field { base, .. } => max_effect_args_expr(*base, ast, host_imports),
        Expr::Call { args, .. } => args
            .iter()
            .map(|&a| max_effect_args_expr(a, ast, host_imports))
            .max()
            .unwrap_or(0),
        Expr::Construct { fields, .. } => fields
            .iter()
            .map(|fi| max_effect_args_expr(fi.value, ast, host_imports))
            .max()
            .unwrap_or(0),
        Expr::Not { operand, .. } | Expr::Neg { operand, .. } => {
            max_effect_args_expr(*operand, ast, host_imports)
        }
        Expr::Binary { left, right, .. } => max_effect_args_expr(*left, ast, host_imports)
            .max(max_effect_args_expr(*right, ast, host_imports)),
        Expr::Int { .. }
        | Expr::Bool { .. }
        | Expr::Null { .. }
        | Expr::Ident(_)
        | Expr::Str(_) => 0,
    }
}

/// 全模块 emit 共享上下文。
struct EmitContext<'a> {
    model: &'a sophia_semantic::SemanticModel,
    lib_index: &'a sophia_hir::AsgIndex,
    host_imports: &'a HostImports,
    import_count: u32,
    func_index: &'a BTreeMap<String, u32>,
    interner: &'a StrInterner,
}

/// 单 callable 的 body emit 上下文。
struct FnEmitter<'a> {
    ast: &'a Ast,
    model: &'a sophia_semantic::SemanticModel,
    host_imports: &'a HostImports,
    import_count: u32,
    func_index: &'a BTreeMap<String, u32>,
    interner: &'a StrInterner,
    /// 变量名 → WASM 局部索引（params 在前，lets 在后）。
    locals: BTreeMap<String, u32>,
    /// 跨调用 propagation 的 outcome 暂存局部索引。
    scratch: u32,
    /// match subject 暂存局部索引。
    subj_scratch: u32,
    /// variant 记录构造的基址暂存局部索引（与 subject 分开，避免 match 内构造时别名）。
    rec_scratch: u32,
    /// repeat 循环计数器暂存局部索引。
    loop_scratch: u32,
    /// host op / ValueWire 暂存局部索引。
    io_a: u32,
    io_b: u32,
    /// host op 实参句柄暂存局部起点。
    host_arg_base: u32,
    /// 当前 callable 的类型推导表（静态分派用，如 `Add` 的 Int/Text）。
    types: sophia_semantic::type_layer::TypeTable,
    instrs: Vec<Instruction<'static>>,
}

fn emit_callable(ast: &Ast, callable: &Callable, ctx: &EmitContext<'_>) -> CodegenResult<Function> {
    let body = callable
        .body
        .as_ref()
        .ok_or_else(|| CodegenError::InvalidInput(format!("`{}` 无 body", callable.name.text)))?;

    // 局部布局：params 0..nparams-1；lets / pattern 绑定 nparams..；scratch + subj_scratch 末两位。
    let nparams = callable.inputs.len() as u32;
    let mut let_names = Vec::new();
    collect_lets(body, &mut let_names);
    let mut locals = BTreeMap::new();
    for (i, p) in callable.inputs.iter().enumerate() {
        locals.insert(p.name.text.clone(), i as u32);
    }
    for (i, n) in let_names.iter().enumerate() {
        locals.insert(n.clone(), nparams + i as u32);
    }
    let scratch = nparams + let_names.len() as u32;
    let subj_scratch = scratch + 1;
    let rec_scratch = scratch + 2;
    let loop_scratch = scratch + 3;
    let io_a = scratch + 4;
    let io_b = scratch + 5;
    let host_arg_base = scratch + 6;
    let host_arg_count = max_effect_arg_count(body, ast, ctx.host_imports);
    let declared_locals = let_names.len() as u32 + 6 + host_arg_count; // lets/bindings + scratch + host args

    // 重算类型表（Table 模式，语义 6.2）：codegen 不要求 analyze_program 暴露它。
    let out = sophia_semantic::type_layer::TypeChecker::new(ctx.model, ast, ctx.lib_index)
        .check_callable(&callable.name.text);

    let mut emitter = FnEmitter {
        ast,
        model: ctx.model,
        host_imports: ctx.host_imports,
        import_count: ctx.import_count,
        func_index: ctx.func_index,
        interner: ctx.interner,
        locals,
        scratch,
        subj_scratch,
        rec_scratch,
        loop_scratch,
        io_a,
        io_b,
        host_arg_base,
        types: out.table,
        instrs: Vec::new(),
    };
    emitter.emit_block(body)?;
    // 尾兜底：fall-through（Unit action 顺序结束）→ Returned(Unit)（复刻解释器 Signal::Next）。
    emitter
        .instrs
        .push(Instruction::Call(emitter.h(helper::MAKE_UNIT)));
    emitter
        .instrs
        .push(Instruction::Call(emitter.h(helper::WRAP_RETURNED)));

    let mut f = Function::new([(declared_locals, ValType::I32)]);
    for ins in &emitter.instrs {
        f.instruction(ins);
    }
    f.instruction(&Instruction::End);
    Ok(f)
}

impl FnEmitter<'_> {
    fn h(&self, relative: u32) -> u32 {
        helper_index(self.import_count, relative)
    }

    fn emit_block(&mut self, block: &Block) -> CodegenResult<()> {
        for stmt in &block.stmts {
            self.emit_stmt(stmt)?;
        }
        Ok(())
    }

    fn emit_stmt(&mut self, stmt: &Stmt) -> CodegenResult<()> {
        match stmt {
            Stmt::Let { name, value, .. } => {
                self.emit_expr(*value)?;
                let idx = self.local_idx(&name.text)?;
                self.instrs.push(Instruction::LocalSet(idx));
                Ok(())
            }
            Stmt::Set { name, value, .. } => {
                self.emit_expr(*value)?;
                let idx = self.local_idx(&name.text)?;
                self.instrs.push(Instruction::LocalSet(idx));
                Ok(())
            }
            Stmt::Return { value, .. } => {
                self.emit_expr(*value)?;
                self.instrs
                    .push(Instruction::Call(self.h(helper::WRAP_RETURNED)));
                self.instrs.push(Instruction::Return);
                Ok(())
            }
            Stmt::If {
                condition,
                consequence,
                alternative,
                ..
            } => {
                self.emit_expr(*condition)?;
                self.instrs
                    .push(Instruction::Call(self.h(helper::GET_BOOL)));
                self.instrs.push(Instruction::If(BlockType::Empty));
                self.emit_block(consequence)?;
                self.instrs.push(Instruction::Else);
                match alternative {
                    Some(ElseBranch::Block(b)) => self.emit_block(b)?,
                    Some(ElseBranch::If(s)) => self.emit_stmt(s)?,
                    None => {}
                }
                self.instrs.push(Instruction::End);
                Ok(())
            }
            Stmt::Expr { value, .. } => {
                self.emit_expr(*value)?;
                self.instrs.push(Instruction::Drop);
                Ok(())
            }
            Stmt::Raise { value, .. } => {
                // raise V { ... }：构造 ErrorValue → 包成 Raised Outcome → return。
                let Expr::Construct { name, fields, .. } = self.ast.expr(*value) else {
                    return Err(CodegenError::InvalidInput(
                        "raise 的值不是 variant 构造".into(),
                    ));
                };
                self.emit_variant_value(&name.text, fields)?;
                self.instrs
                    .push(Instruction::Call(self.h(helper::WRAP_RAISED)));
                self.instrs.push(Instruction::Return);
                Ok(())
            }
            Stmt::Match { subject, arms, .. } => self.emit_match(*subject, arms),
            Stmt::Repeat { count, body, .. } => {
                // repeat n times { body }：n.max(0) 次。body 内 return/raise 经 WASM `return`
                // 直接退出函数（Outcome ABI），与解释器一致。用一个倒计数 i32 局部（rec_scratch
                // 在此语句不构造记录，可安全复用作计数器；body 内构造会重分配，故不与之冲突——
                // 但为稳妥，repeat 计数用独立 loop_scratch 局部）。
                let counter = self.loop_scratch;
                self.emit_expr(*count)?;
                self.instrs.push(Instruction::Call(self.h(helper::GET_INT)));
                self.instrs.push(Instruction::I32WrapI64);
                self.instrs.push(Instruction::LocalSet(counter));
                self.instrs.push(Instruction::Block(BlockType::Empty));
                self.instrs.push(Instruction::Loop(BlockType::Empty));
                // if counter <= 0: break
                self.instrs.push(Instruction::LocalGet(counter));
                self.instrs.push(Instruction::I32Const(0));
                self.instrs.push(Instruction::I32LeS);
                self.instrs.push(Instruction::BrIf(1));
                self.emit_block(body)?;
                // counter--
                self.instrs.push(Instruction::LocalGet(counter));
                self.instrs.push(Instruction::I32Const(1));
                self.instrs.push(Instruction::I32Sub);
                self.instrs.push(Instruction::LocalSet(counter));
                self.instrs.push(Instruction::Br(0));
                self.instrs.push(Instruction::End); // loop
                self.instrs.push(Instruction::End); // block
                Ok(())
            }
            Stmt::Print { value, .. } => {
                // print：Console.Write effect。值在运行时是 Text 值（intent 擦除）；取其字节经
                // host import console_write 输出。与解释器 `host.console_write(&v.to_string())` 一致
                // （起步子集 print 的值建模为 Text / Sanitized<Text>，to_string 即其文本）。
                self.emit_expr(*value)?;
                self.instrs.push(Instruction::LocalTee(self.rec_scratch));
                self.instrs
                    .push(Instruction::Call(self.h(helper::GET_TEXT_PTR)));
                self.instrs.push(Instruction::LocalGet(self.rec_scratch));
                self.instrs
                    .push(Instruction::Call(self.h(helper::GET_TEXT_LEN)));
                self.instrs.push(Instruction::Call(CONSOLE_WRITE_IMPORT));
                Ok(())
            }
        }
    }

    /// emit `match subject { arms }`：复刻解释器逐臂尝试匹配、首个命中执行其 body。
    ///
    /// 把 subject 句柄存入 `subj_scratch`，按臂顺序生成「条件→ if 块」嵌套链；穷尽性由语义层
    /// 保证（最后无兜底——若无臂命中是编译期已排除的情况，与解释器防御性 `Signal::Next` 一致：
    /// 这里靠尾兜底 Returned(Unit)）。
    fn emit_match(
        &mut self,
        subject: ExprId,
        arms: &[sophia_syntax::MatchArm],
    ) -> CodegenResult<()> {
        self.emit_expr(subject)?;
        self.instrs.push(Instruction::LocalSet(self.subj_scratch));
        let mut depth = 0u32;
        for arm in arms {
            self.emit_pattern_test(&arm.pattern)?; // 压入 i32 条件
            self.instrs.push(Instruction::If(BlockType::Empty));
            self.emit_pattern_bindings(&arm.pattern)?;
            self.emit_block(&arm.body)?;
            self.instrs.push(Instruction::Else);
            depth += 1;
        }
        for _ in 0..depth {
            self.instrs.push(Instruction::End);
        }
        Ok(())
    }

    /// emit 一个 pattern 的匹配条件（结果 i32 压栈：1=匹配）。subject 在 `subj_scratch`。
    fn emit_pattern_test(&mut self, pattern: &Pattern) -> CodegenResult<()> {
        match pattern {
            Pattern::Bool { value, .. } => {
                // tag==Bool && payload==value
                self.push_subj_tag();
                self.instrs.push(Instruction::I32Const(tag::BOOL));
                self.instrs.push(Instruction::I32Eq);
                self.instrs.push(Instruction::LocalGet(self.subj_scratch));
                self.instrs
                    .push(Instruction::Call(self.h(helper::GET_BOOL)));
                self.instrs.push(Instruction::I32Const(i32::from(*value)));
                self.instrs.push(Instruction::I32Eq);
                self.instrs.push(Instruction::I32And);
                Ok(())
            }
            Pattern::Null { .. } => {
                self.push_subj_tag();
                self.instrs.push(Instruction::I32Const(tag::NULL));
                self.instrs.push(Instruction::I32Eq);
                Ok(())
            }
            // 类型 pattern：标量类型名按 tag；entity 类型名按 tag==Entity + 记录名匹配；
            // state 类型名按 tag==State + state 名匹配。
            Pattern::Type { ty, .. } => {
                if let Some(want) = scalar_type_tag(&ty.text) {
                    self.push_subj_tag();
                    self.instrs.push(Instruction::I32Const(want));
                    self.instrs.push(Instruction::I32Eq);
                    Ok(())
                } else if self.model.entities.contains_key(&ty.text) {
                    let (ptr, len) = self.interner.lookup(&ty.text);
                    self.push_subj_tag();
                    self.instrs.push(Instruction::I32Const(tag::ENTITY));
                    self.instrs.push(Instruction::I32Eq);
                    self.instrs.push(Instruction::LocalGet(self.subj_scratch));
                    self.instrs.push(Instruction::I32Const(ptr));
                    self.instrs.push(Instruction::I32Const(len));
                    self.instrs
                        .push(Instruction::Call(self.h(helper::REC_NAME_EQ)));
                    self.instrs.push(Instruction::I32And);
                    Ok(())
                } else if self.model.states.contains_key(&ty.text) {
                    let (ptr, len) = self.interner.lookup(&ty.text);
                    self.push_subj_tag();
                    self.instrs.push(Instruction::I32Const(tag::STATE));
                    self.instrs.push(Instruction::I32Eq);
                    self.instrs.push(Instruction::LocalGet(self.subj_scratch));
                    self.instrs.push(Instruction::I32Const(ptr));
                    self.instrs.push(Instruction::I32Const(len));
                    self.instrs
                        .push(Instruction::Call(self.h(helper::STATE_NAME_EQ)));
                    self.instrs.push(Instruction::I32And);
                    Ok(())
                } else {
                    Err(unsupported(
                        "match 类型 pattern 类型名未知（Text 待后续增量）",
                    ))
                }
            }
            // variant pattern：tag==ErrorValue && 记录名 == variant 名。
            Pattern::Variant { variant, .. } => {
                let (ptr, len) = self.interner.lookup(&variant.text);
                self.push_subj_tag();
                self.instrs.push(Instruction::I32Const(tag::ERROR_VALUE));
                self.instrs.push(Instruction::I32Eq);
                self.instrs.push(Instruction::LocalGet(self.subj_scratch));
                self.instrs.push(Instruction::I32Const(ptr));
                self.instrs.push(Instruction::I32Const(len));
                self.instrs
                    .push(Instruction::Call(self.h(helper::REC_NAME_EQ)));
                self.instrs.push(Instruction::I32And);
                Ok(())
            }
            // state 值 pattern `StateName.Value`：tag==State && state 名 + value 名均匹配。
            Pattern::State { head, value, .. } => {
                let (sptr, slen) = self.interner.lookup(&head.text);
                let (vptr, vlen) = self.interner.lookup(&value.text);
                self.push_subj_tag();
                self.instrs.push(Instruction::I32Const(tag::STATE));
                self.instrs.push(Instruction::I32Eq);
                self.instrs.push(Instruction::LocalGet(self.subj_scratch));
                self.instrs.push(Instruction::I32Const(sptr));
                self.instrs.push(Instruction::I32Const(slen));
                self.instrs
                    .push(Instruction::Call(self.h(helper::STATE_NAME_EQ)));
                self.instrs.push(Instruction::I32And);
                self.instrs.push(Instruction::LocalGet(self.subj_scratch));
                self.instrs.push(Instruction::I32Const(vptr));
                self.instrs.push(Instruction::I32Const(vlen));
                self.instrs
                    .push(Instruction::Call(self.h(helper::STATE_VALUE_EQ)));
                self.instrs.push(Instruction::I32And);
                Ok(())
            }
        }
    }

    /// emit pattern 绑定（type binding = 整个 subject；variant 字段 = 按名取记录字段值）。
    fn emit_pattern_bindings(&mut self, pattern: &Pattern) -> CodegenResult<()> {
        match pattern {
            Pattern::Type { binding, .. } => {
                let idx = self.local_idx(&binding.text)?;
                self.instrs.push(Instruction::LocalGet(self.subj_scratch));
                self.instrs.push(Instruction::LocalSet(idx));
                Ok(())
            }
            Pattern::Variant { fields, .. } => {
                for fb in fields {
                    let (ptr, len) = self.interner.lookup(&fb.text);
                    let idx = self.local_idx(&fb.text)?;
                    self.instrs.push(Instruction::LocalGet(self.subj_scratch));
                    self.instrs.push(Instruction::I32Const(ptr));
                    self.instrs.push(Instruction::I32Const(len));
                    self.instrs
                        .push(Instruction::Call(self.h(helper::REC_FIELD)));
                    self.instrs.push(Instruction::LocalSet(idx));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn push_subj_tag(&mut self) {
        self.instrs.push(Instruction::LocalGet(self.subj_scratch));
        self.instrs
            .push(Instruction::Call(self.h(helper::VALUE_TAG)));
    }

    fn emit_expr(&mut self, id: ExprId) -> CodegenResult<()> {
        match self.ast.expr(id) {
            Expr::Int { text, .. } => {
                let v: i64 = text
                    .parse()
                    .map_err(|_| CodegenError::InvalidInput(format!("非法整数字面量 `{text}`")))?;
                self.instrs.push(Instruction::I64Const(v));
                self.instrs
                    .push(Instruction::Call(self.h(helper::MAKE_INT)));
                Ok(())
            }
            Expr::Bool { value, .. } => {
                self.instrs.push(Instruction::I32Const(i32::from(*value)));
                self.instrs
                    .push(Instruction::Call(self.h(helper::MAKE_BOOL)));
                Ok(())
            }
            Expr::Null { .. } => {
                self.instrs
                    .push(Instruction::Call(self.h(helper::MAKE_NULL)));
                Ok(())
            }
            Expr::Ident(name) => {
                let idx = self.local_idx(&name.text)?;
                self.instrs.push(Instruction::LocalGet(idx));
                Ok(())
            }
            Expr::Not { operand, .. } => {
                self.emit_expr(*operand)?;
                self.instrs
                    .push(Instruction::Call(self.h(helper::GET_BOOL)));
                self.instrs.push(Instruction::I32Eqz);
                self.instrs
                    .push(Instruction::Call(self.h(helper::MAKE_BOOL)));
                Ok(())
            }
            Expr::Neg { operand, .. } => {
                self.emit_expr(*operand)?;
                self.instrs.push(Instruction::Call(self.h(helper::GET_INT)));
                self.instrs.push(Instruction::I64Const(-1));
                self.instrs.push(Instruction::I64Mul);
                self.instrs
                    .push(Instruction::Call(self.h(helper::MAKE_INT)));
                Ok(())
            }
            Expr::Binary {
                op, left, right, ..
            } => self.emit_binary(*op, *left, *right, id),
            Expr::Call { callee, args, .. } => self.emit_call(&callee.text, args),
            Expr::Field { base, field, .. } => self.emit_field(*base, &field.text),
            Expr::Str(s) => {
                let (ptr, len) = self.interner.lookup(&s.value);
                self.instrs.push(Instruction::I32Const(ptr));
                self.instrs.push(Instruction::I32Const(len));
                self.instrs
                    .push(Instruction::Call(self.h(helper::MAKE_TEXT)));
                Ok(())
            }
            Expr::List { .. } => Err(unsupported("list（待后续增量）")),
            Expr::MethodCall {
                base, method, args, ..
            } => self.emit_method_call(*base, &method.text, args),
            Expr::Construct { name, fields, .. } => {
                // variant 构造（被返回的 `one of` 失败成员）→ ErrorValue 值。
                if self.model.variants.contains_key(&name.text) {
                    self.emit_variant_value(&name.text, fields)
                } else if self.model.entities.contains_key(&name.text) {
                    self.emit_entity_value(&name.text, fields)
                } else {
                    Err(unsupported(
                        "非 entity / variant 构造（如 transition 调用，待后续增量）",
                    ))
                }
            }
        }
    }

    fn emit_binary(
        &mut self,
        op: BinOp,
        left: ExprId,
        right: ExprId,
        whole: ExprId,
    ) -> CodegenResult<()> {
        use BinOp::*;
        match op {
            And | Or => {
                self.emit_expr(left)?;
                self.instrs
                    .push(Instruction::Call(self.h(helper::GET_BOOL)));
                self.emit_expr(right)?;
                self.instrs
                    .push(Instruction::Call(self.h(helper::GET_BOOL)));
                self.instrs.push(if op == And {
                    Instruction::I32And
                } else {
                    Instruction::I32Or
                });
                self.instrs
                    .push(Instruction::Call(self.h(helper::MAKE_BOOL)));
                Ok(())
            }
            Eq | Ne => {
                // value_eq 仅覆盖标量结构相等（Int/Bool/Null/Unit）；非标量操作数（Text/List/
                // Entity/State/ErrorValue）的相等待后续增量，诚实拒绝（避免误判相等）。
                self.require_scalar_eq(left)?;
                self.require_scalar_eq(right)?;
                self.emit_expr(left)?;
                self.emit_expr(right)?;
                self.instrs
                    .push(Instruction::Call(self.h(helper::VALUE_EQ)));
                if op == Ne {
                    self.instrs.push(Instruction::I32Eqz);
                }
                self.instrs
                    .push(Instruction::Call(self.h(helper::MAKE_BOOL)));
                Ok(())
            }
            Lt | Le | Gt | Ge => {
                self.emit_expr(left)?;
                self.instrs.push(Instruction::Call(self.h(helper::GET_INT)));
                self.emit_expr(right)?;
                self.instrs.push(Instruction::Call(self.h(helper::GET_INT)));
                self.instrs.push(match op {
                    Lt => Instruction::I64LtS,
                    Le => Instruction::I64LeS,
                    Gt => Instruction::I64GtS,
                    Ge => Instruction::I64GeS,
                    _ => unreachable!(),
                });
                self.instrs
                    .push(Instruction::Call(self.h(helper::MAKE_BOOL)));
                Ok(())
            }
            Add => {
                // 静态分派：Int 加法 / Text 拼接（List 追加待后续增量）。
                use sophia_semantic::ty::Ty;
                match self.types.get(whole).map(|t| t.strip_intent()) {
                    Some(Ty::Int) => self.emit_int_binop(left, right, Instruction::I64Add),
                    Some(Ty::Text) => {
                        self.emit_expr(left)?;
                        self.emit_expr(right)?;
                        self.instrs
                            .push(Instruction::Call(self.h(helper::TEXT_CONCAT)));
                        Ok(())
                    }
                    _ => Err(unsupported("Add 非 Int / Text（List 追加待后续增量）")),
                }
            }
            Sub => self.emit_int_binop(left, right, Instruction::I64Sub),
            Mul => self.emit_int_binop(left, right, Instruction::I64Mul),
        }
    }

    fn emit_int_binop(
        &mut self,
        left: ExprId,
        right: ExprId,
        op: Instruction<'static>,
    ) -> CodegenResult<()> {
        self.emit_expr(left)?;
        self.instrs.push(Instruction::Call(self.h(helper::GET_INT)));
        self.emit_expr(right)?;
        self.instrs.push(Instruction::Call(self.h(helper::GET_INT)));
        self.instrs.push(op);
        self.instrs
            .push(Instruction::Call(self.h(helper::MAKE_INT)));
        Ok(())
    }

    /// emit 库 effect op：`Family.Op(args)`。import 表由 registry + 实际使用面派生；不存在
    /// `File`/`Http` 专用分支。
    fn emit_method_call(
        &mut self,
        base: ExprId,
        method: &str,
        args: &[ExprId],
    ) -> CodegenResult<()> {
        let Expr::Ident(root) = self.ast.expr(base) else {
            return Err(unsupported(
                "方法调用（非库特殊根，list.append 待后续增量）",
            ));
        };
        let Some(import) = self
            .host_imports
            .ops
            .get(&(root.text.clone(), method.to_string()))
            .cloned()
        else {
            return Err(unsupported(&format!(
                "方法调用 `{}`.`{method}`（非库 op，待后续增量）",
                root.text
            )));
        };
        self.emit_effect_op(&import.contract, import.index, args)
    }

    fn emit_effect_op(
        &mut self,
        contract: &OpContract,
        import_index: u32,
        args: &[ExprId],
    ) -> CodegenResult<()> {
        if args.len() != contract.params.len() {
            return Err(CodegenError::InvalidInput(format!(
                "{}.{} 需 {} 实参，实际 {}",
                contract.family,
                contract.op,
                contract.params.len(),
                args.len()
            )));
        }
        for (i, &arg) in args.iter().enumerate() {
            self.emit_expr(arg)?;
            self.instrs
                .push(Instruction::LocalSet(self.host_arg_base + i as u32));
        }

        self.emit_wire_args_len(contract)?;
        self.instrs.push(Instruction::Call(self.h(helper::ALLOC)));
        self.instrs.push(Instruction::LocalSet(self.io_a)); // args_ptr
        self.instrs.push(Instruction::LocalGet(self.io_a));
        self.instrs
            .push(Instruction::I32Const(contract.params.len() as i32));
        self.instrs.push(Instruction::I32Store(mem(0, I32_MEM)));
        self.instrs.push(Instruction::LocalGet(self.io_a));
        self.instrs.push(Instruction::I32Const(4));
        self.instrs.push(Instruction::I32Add);
        self.instrs.push(Instruction::LocalSet(self.io_b)); // cursor
        for (i, param) in contract.params.iter().enumerate() {
            let scalar = wire_scalar(param)?;
            self.emit_wire_value(self.host_arg_base + i as u32, scalar)?;
        }

        self.instrs.push(Instruction::LocalGet(self.io_a));
        self.emit_wire_args_len(contract)?;
        self.instrs.push(Instruction::Call(import_index));
        self.instrs.push(Instruction::LocalSet(self.scratch)); // result_len
        self.instrs.push(Instruction::LocalGet(self.scratch));
        self.instrs.push(Instruction::Call(self.h(helper::ALLOC)));
        self.instrs.push(Instruction::LocalSet(self.io_b)); // result_ptr
        self.instrs.push(Instruction::LocalGet(self.io_b));
        self.instrs.push(Instruction::Call(READ_COPY_IMPORT));
        self.emit_wire_result(wire_scalar(&contract.returns)?)
    }

    fn emit_wire_args_len(&mut self, contract: &OpContract) -> CodegenResult<()> {
        self.instrs.push(Instruction::I32Const(4)); // argc
        for (i, param) in contract.params.iter().enumerate() {
            match wire_scalar(param)? {
                Scalar::Unit => self.instrs.push(Instruction::I32Const(1)),
                Scalar::Bool => self.instrs.push(Instruction::I32Const(2)),
                Scalar::Int => self.instrs.push(Instruction::I32Const(9)),
                Scalar::Text => {
                    self.instrs.push(Instruction::I32Const(5));
                    self.instrs
                        .push(Instruction::LocalGet(self.host_arg_base + i as u32));
                    self.instrs
                        .push(Instruction::Call(self.h(helper::GET_TEXT_LEN)));
                    self.instrs.push(Instruction::I32Add);
                }
            }
            self.instrs.push(Instruction::I32Add);
        }
        Ok(())
    }

    fn emit_wire_value(&mut self, value_local: u32, scalar: Scalar) -> CodegenResult<()> {
        self.instrs.push(Instruction::LocalGet(self.io_b));
        self.instrs
            .push(Instruction::I32Const(wire_tag(scalar) as i32));
        self.instrs.push(Instruction::I32Store8(mem(0, 0)));
        self.bump_cursor(1);
        match scalar {
            Scalar::Unit => {}
            Scalar::Bool => {
                self.instrs.push(Instruction::LocalGet(self.io_b));
                self.instrs.push(Instruction::LocalGet(value_local));
                self.instrs
                    .push(Instruction::Call(self.h(helper::GET_BOOL)));
                self.instrs.push(Instruction::I32Store8(mem(0, 0)));
                self.bump_cursor(1);
            }
            Scalar::Int => {
                self.instrs.push(Instruction::LocalGet(self.io_b));
                self.instrs.push(Instruction::LocalGet(value_local));
                self.instrs.push(Instruction::Call(self.h(helper::GET_INT)));
                self.instrs.push(Instruction::I64Store(mem(0, 0)));
                self.bump_cursor(8);
            }
            Scalar::Text => {
                self.instrs.push(Instruction::LocalGet(self.io_b));
                self.instrs.push(Instruction::LocalGet(value_local));
                self.instrs
                    .push(Instruction::Call(self.h(helper::GET_TEXT_LEN)));
                self.instrs.push(Instruction::I32Store(mem(0, I32_MEM)));
                self.bump_cursor(4);
                self.emit_copy_text_bytes(value_local)?;
            }
        }
        Ok(())
    }

    fn emit_copy_text_bytes(&mut self, value_local: u32) -> CodegenResult<()> {
        self.instrs.push(Instruction::I32Const(0));
        self.instrs.push(Instruction::LocalSet(self.loop_scratch));
        self.instrs.push(Instruction::Block(BlockType::Empty));
        self.instrs.push(Instruction::Loop(BlockType::Empty));
        self.instrs.push(Instruction::LocalGet(self.loop_scratch));
        self.instrs.push(Instruction::LocalGet(value_local));
        self.instrs
            .push(Instruction::Call(self.h(helper::GET_TEXT_LEN)));
        self.instrs.push(Instruction::I32GeU);
        self.instrs.push(Instruction::BrIf(1));
        self.instrs.push(Instruction::LocalGet(self.io_b));
        self.instrs.push(Instruction::LocalGet(self.loop_scratch));
        self.instrs.push(Instruction::I32Add);
        self.instrs.push(Instruction::LocalGet(value_local));
        self.instrs
            .push(Instruction::Call(self.h(helper::GET_TEXT_PTR)));
        self.instrs.push(Instruction::LocalGet(self.loop_scratch));
        self.instrs.push(Instruction::I32Add);
        self.instrs.push(Instruction::I32Load8U(mem(0, 0)));
        self.instrs.push(Instruction::I32Store8(mem(0, 0)));
        self.instrs.push(Instruction::LocalGet(self.loop_scratch));
        self.instrs.push(Instruction::I32Const(1));
        self.instrs.push(Instruction::I32Add);
        self.instrs.push(Instruction::LocalSet(self.loop_scratch));
        self.instrs.push(Instruction::Br(0));
        self.instrs.push(Instruction::End);
        self.instrs.push(Instruction::End);
        self.instrs.push(Instruction::LocalGet(self.io_b));
        self.instrs.push(Instruction::LocalGet(value_local));
        self.instrs
            .push(Instruction::Call(self.h(helper::GET_TEXT_LEN)));
        self.instrs.push(Instruction::I32Add);
        self.instrs.push(Instruction::LocalSet(self.io_b));
        Ok(())
    }

    fn emit_wire_result(&mut self, scalar: Scalar) -> CodegenResult<()> {
        self.assert_wire_tag(self.io_b, scalar);
        match scalar {
            Scalar::Unit => self
                .instrs
                .push(Instruction::Call(self.h(helper::MAKE_UNIT))),
            Scalar::Bool => {
                self.instrs.push(Instruction::LocalGet(self.io_b));
                self.instrs.push(Instruction::I32Load8U(mem(1, 0)));
                self.instrs
                    .push(Instruction::Call(self.h(helper::MAKE_BOOL)));
            }
            Scalar::Int => {
                self.instrs.push(Instruction::LocalGet(self.io_b));
                self.instrs.push(Instruction::I64Load(mem(1, 0)));
                self.instrs
                    .push(Instruction::Call(self.h(helper::MAKE_INT)));
            }
            Scalar::Text => {
                self.instrs.push(Instruction::LocalGet(self.io_b));
                self.instrs.push(Instruction::I32Const(5));
                self.instrs.push(Instruction::I32Add);
                self.instrs.push(Instruction::LocalGet(self.io_b));
                self.instrs.push(Instruction::I32Load(mem(1, 0)));
                self.instrs
                    .push(Instruction::Call(self.h(helper::MAKE_TEXT)));
            }
        }
        Ok(())
    }

    fn assert_wire_tag(&mut self, ptr_local: u32, scalar: Scalar) {
        self.instrs.push(Instruction::LocalGet(ptr_local));
        self.instrs.push(Instruction::I32Load8U(mem(0, 0)));
        self.instrs
            .push(Instruction::I32Const(wire_tag(scalar) as i32));
        self.instrs.push(Instruction::I32Ne);
        self.instrs.push(Instruction::If(BlockType::Empty));
        self.instrs.push(Instruction::Unreachable);
        self.instrs.push(Instruction::End);
    }

    fn bump_cursor(&mut self, n: i32) {
        self.instrs.push(Instruction::LocalGet(self.io_b));
        self.instrs.push(Instruction::I32Const(n));
        self.instrs.push(Instruction::I32Add);
        self.instrs.push(Instruction::LocalSet(self.io_b));
    }

    fn emit_call(&mut self, callee: &str, args: &[ExprId]) -> CodegenResult<()> {
        if callee == "to_text" {
            return Err(unsupported("to_text（Text 待后续增量）"));
        }
        let Some(&idx) = self.func_index.get(callee) else {
            return Err(CodegenError::InvalidInput(format!(
                "未知调用目标 `{callee}`"
            )));
        };
        if !self.model.callables.contains_key(callee) {
            return Err(CodegenError::InvalidInput(format!(
                "`{callee}` 不是 callable"
            )));
        }
        // 实参逐个求值（顺序与解释器一致）。
        for &a in args {
            self.emit_expr(a)?;
        }
        self.instrs.push(Instruction::Call(idx));
        // propagation：raised 则原样上抛；returned 则取其 value。
        self.instrs.push(Instruction::LocalTee(self.scratch));
        self.instrs
            .push(Instruction::Call(self.h(helper::OUTCOME_KIND)));
        self.instrs.push(Instruction::I32Const(outcome::RAISED));
        self.instrs.push(Instruction::I32Eq);
        self.instrs.push(Instruction::If(BlockType::Empty));
        self.instrs.push(Instruction::LocalGet(self.scratch));
        self.instrs.push(Instruction::Return);
        self.instrs.push(Instruction::End);
        self.instrs.push(Instruction::LocalGet(self.scratch));
        self.instrs
            .push(Instruction::Call(self.h(helper::OUTCOME_VALUE)));
        Ok(())
    }

    fn local_idx(&self, name: &str) -> CodegenResult<u32> {
        self.locals
            .get(name)
            .copied()
            .ok_or_else(|| CodegenError::InvalidInput(format!("未绑定变量 `{name}`")))
    }

    /// 要求表达式静态类型可用 `value_eq` 比较（Int/Bool/Null/Unit/Text），否则 `NotYetImplemented`。
    fn require_scalar_eq(&self, id: ExprId) -> CodegenResult<()> {
        use sophia_semantic::ty::Ty;
        match self.types.get(id).map(|t| t.strip_intent()) {
            Some(Ty::Int | Ty::Bool | Ty::Null | Ty::Unit | Ty::Text) | None => Ok(()),
            _ => Err(unsupported(
                "==/!= 非可比类型（List/Entity/State/ErrorValue 相等待后续增量）",
            )),
        }
    }

    /// emit 一个 error variant 记录值（被返回 / raise 的成员）→ 堆上 ErrorValue，句柄压栈。
    fn emit_variant_value(
        &mut self,
        variant: &str,
        fields: &[sophia_syntax::FieldInit],
    ) -> CodegenResult<()> {
        self.emit_record_value(tag::ERROR_VALUE, variant, fields)
    }

    /// emit 一个 entity 记录值 → 堆上 Entity，句柄压栈（与 ErrorValue 同布局，tag 不同）。
    fn emit_entity_value(
        &mut self,
        name: &str,
        fields: &[sophia_syntax::FieldInit],
    ) -> CodegenResult<()> {
        self.emit_record_value(tag::ENTITY, name, fields)
    }

    /// emit 一个具名记录值（ErrorValue / Entity 共用）→ 堆值，句柄压栈。
    ///
    /// 布局见 abi.rs：`[tag][name_ptr][name_len][nfields]` + 各字段 `[key_ptr][key_len][val]`，
    /// **字段按 key 字典序**（与解释器 `BTreeMap` 一致 → 结构相等可逐位比较 + 字节确定）。
    fn emit_record_value(
        &mut self,
        record_tag: i32,
        name: &str,
        fields: &[sophia_syntax::FieldInit],
    ) -> CodegenResult<()> {
        // 字段按字典序（与值布局约定一致）。
        let mut ordered: Vec<&sophia_syntax::FieldInit> = fields.iter().collect();
        ordered.sort_by(|a, b| a.name.text.cmp(&b.name.text));
        let nfields = ordered.len() as i32;

        // 嵌套记录构造会与本记录基址 rec_scratch 别名（单暂存槽），暂不支持；诚实拒绝。
        for fi in &ordered {
            if let Expr::Construct { name, .. } = self.ast.expr(fi.value) {
                if self.model.variants.contains_key(&name.text)
                    || self.model.entities.contains_key(&name.text)
                {
                    return Err(unsupported("嵌套记录构造字段（待后续增量）"));
                }
            }
        }

        let rec = self.rec_scratch;
        // alloc(size)
        let size = abi::REC_HEADER_SIZE + nfields * abi::REC_FIELD_SIZE;
        self.instrs.push(Instruction::I32Const(size));
        self.instrs.push(Instruction::Call(self.h(helper::ALLOC)));
        self.instrs.push(Instruction::LocalSet(rec));
        // header: tag
        self.instrs.push(Instruction::LocalGet(rec));
        self.instrs.push(Instruction::I32Const(record_tag));
        self.instrs
            .push(Instruction::I32Store(mem(abi::OFF_TAG, I32_MEM)));
        // name_ptr / name_len
        let (nptr, nlen) = self.interner.lookup(name);
        self.instrs.push(Instruction::LocalGet(rec));
        self.instrs.push(Instruction::I32Const(nptr));
        self.instrs
            .push(Instruction::I32Store(mem(abi::OFF_REC_NAME_PTR, I32_MEM)));
        self.instrs.push(Instruction::LocalGet(rec));
        self.instrs.push(Instruction::I32Const(nlen));
        self.instrs
            .push(Instruction::I32Store(mem(abi::OFF_REC_NAME_LEN, I32_MEM)));
        // nfields
        self.instrs.push(Instruction::LocalGet(rec));
        self.instrs.push(Instruction::I32Const(nfields));
        self.instrs
            .push(Instruction::I32Store(mem(abi::OFF_REC_NFIELDS, I32_MEM)));
        // 各字段：key_ptr / key_len / val_handle
        for (i, fi) in ordered.iter().enumerate() {
            let field_off = abi::REC_HEADER_SIZE as u64 + i as u64 * abi::REC_FIELD_SIZE as u64;
            let (kptr, klen) = self.interner.lookup(&fi.name.text);
            // key_ptr
            self.instrs.push(Instruction::LocalGet(rec));
            self.instrs.push(Instruction::I32Const(kptr));
            self.instrs.push(Instruction::I32Store(mem(
                field_off + abi::OFF_FIELD_KEY_PTR,
                I32_MEM,
            )));
            // key_len
            self.instrs.push(Instruction::LocalGet(rec));
            self.instrs.push(Instruction::I32Const(klen));
            self.instrs.push(Instruction::I32Store(mem(
                field_off + abi::OFF_FIELD_KEY_LEN,
                I32_MEM,
            )));
            // val_handle = eval(field value)
            self.instrs.push(Instruction::LocalGet(rec));
            self.emit_expr(fi.value)?;
            self.instrs.push(Instruction::I32Store(mem(
                field_off + abi::OFF_FIELD_VAL,
                I32_MEM,
            )));
        }
        // 句柄压栈
        self.instrs.push(Instruction::LocalGet(rec));
        Ok(())
    }

    /// emit 字段访问：`StateName.Value`（状态值字面）或 `entity.field`（记录字段读取）。
    fn emit_field(&mut self, base: ExprId, field: &str) -> CodegenResult<()> {
        // 状态值字面 `StateName.Value`：base 是命名 state 的标识符。
        if let Expr::Ident(bident) = self.ast.expr(base) {
            if let Some(state) = self.model.states.get(&bident.text) {
                if state.has_value(field) {
                    let (sptr, slen) = self.interner.lookup(&bident.text);
                    let (vptr, vlen) = self.interner.lookup(field);
                    self.instrs.push(Instruction::I32Const(sptr));
                    self.instrs.push(Instruction::I32Const(slen));
                    self.instrs.push(Instruction::I32Const(vptr));
                    self.instrs.push(Instruction::I32Const(vlen));
                    self.instrs
                        .push(Instruction::Call(self.h(helper::MAKE_STATE)));
                    return Ok(());
                }
            }
        }
        // entity 字段读取 / Text.length 伪字段：按 base 静态类型分派。
        use sophia_semantic::ty::Ty;
        match self.types.get(base).map(|t| t.strip_intent()) {
            Some(Ty::Entity(_)) => {
                let (kptr, klen) = self.interner.lookup(field);
                self.emit_expr(base)?;
                self.instrs.push(Instruction::I32Const(kptr));
                self.instrs.push(Instruction::I32Const(klen));
                self.instrs
                    .push(Instruction::Call(self.h(helper::REC_FIELD)));
                Ok(())
            }
            Some(Ty::Text) if field == "length" => {
                self.emit_expr(base)?;
                self.instrs
                    .push(Instruction::Call(self.h(helper::TEXT_LENGTH)));
                Ok(())
            }
            _ => Err(unsupported(
                "字段访问 base 类型暂不支持（List 等待后续增量）",
            )),
        }
    }
}

/// 标量类型名 → 值 tag（用于 match 类型 pattern）。非标量返回 `None`。
fn scalar_type_tag(name: &str) -> Option<i32> {
    match name {
        "Unit" => Some(tag::UNIT),
        "Bool" => Some(tag::BOOL),
        "Int" => Some(tag::INT),
        "Null" => Some(tag::NULL),
        "Text" => Some(tag::TEXT),
        _ => None,
    }
}

fn unsupported(what: &str) -> CodegenError {
    CodegenError::NotYetImplemented(what.to_string())
}
