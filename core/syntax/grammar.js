/**
 * Sophia-Core 语法（Tree-sitter grammar）
 *
 * 覆盖 docs/language_implementation.md 第十六节 的起步子集：
 *   顶层节点：domain / entity / state / transition / error / capability / action / task / effect
 *   类型：标量、list of T、one of {..}、schema of T、Intent<T>、entity/state 引用
 *   Body 子语言：let / set / return / raise / if-else / match / repeat / print 与受限表达式
 *
 * Semantic Assist 字段（meaning / not / purpose / because / examples / anti_patterns /
 * plan / repair_notes）在语法层被解析为独立节点，便于 strip-assist 等价门禁在上层移除。
 *
 * 版本对齐：tree-sitter CLI 0.26.x / ABI 15 / tree-sitter crate 0.26。
 */

module.exports = grammar({
  name: 'sophia',

  word: $ => $.identifier,

  extras: $ => [/\s/, $.comment],

  rules: {
    source_file: $ => repeat($._definition),

    comment: $ => token(choice(
      seq('//', /[^\n]*/),
      seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/'),
    )),

    _definition: $ => choice(
      $.domain_def,
      $.entity_def,
      $.state_def,
      $.transition_def,
      $.error_def,
      $.capability_def,
      $.action_def,
      $.task_def,
      $.effect_def,
    ),

    // ---- 标识符与字面量 ----
    identifier: $ => /[A-Za-z_][A-Za-z0-9_]*/,
    string: $ => token(seq('"', repeat(choice(/[^"\\]/, /\\./)), '"')),
    int: $ => token(/-?[0-9]+/),
    bool: $ => choice('true', 'false'),

    // ---- 类型 ----
    // 规则：`<>` 专属 Intent Type（intent_type）；结构类型用 `of` 关键字族
    //（list of / one of / schema of）；裸名为标量 / 渐进 / 具名类型。
    // 见 docs/type_system.md。
    type: $ => choice(
      $.intent_type,
      $.list_of,
      $.one_of,
      $.schema_of,
      $.named_type,
    ),
    named_type: $ => $.identifier,
    // Intent 包装：`Raw<T>` 等 9 个 intent。`<>` 仅用于此。
    intent_type: $ => seq(
      field('head', $.identifier),
      '<',
      field('arg', $.type),
      '>',
    ),
    // 列表：`list of T`。
    list_of: $ => seq('list', 'of', field('elem', $.type)),
    // 联合：`one of { A, B, ... }`，成员是类型（标量 / Null / entity / state / error variant）。
    one_of: $ => seq(
      'one', 'of', '{',
      field('member', $.type),
      repeat(seq(',', field('member', $.type))),
      optional(','),
      '}',
    ),
    // 渐进：`schema of T`。
    schema_of: $ => seq('schema', 'of', field('arg', $.type)),

    // ---- Semantic Assist 字段（不决定语义） ----
    assist_field: $ => choice(
      seq(field('key', choice('meaning', 'purpose', 'because')), ':', field('value', $.string)),
      seq(field('key', choice('not', 'examples', 'anti_patterns', 'plan', 'repair_notes')),
          ':', field('value', $.string_list)),
    ),
    string_list: $ => repeat1($.string),

    // ============ domain ============
    domain_def: $ => seq(
      'domain',
      field('name', $.identifier),
      field('body', $.domain_body),
    ),
    domain_body: $ => seq('{', repeat($.assist_field), '}'),

    // ============ entity ============
    entity_def: $ => seq(
      'entity',
      field('name', $.identifier),
      field('body', $.entity_body),
    ),
    entity_body: $ => seq('{', repeat($._entity_member), '}'),
    _entity_member: $ => choice(
      $.assist_field,
      $.fields_block,
      $.invariants_block,
      $.semantic_identity_block,
      $.evolution_block,
    ),

    fields_block: $ => seq('fields', '{', repeat($.field_decl), '}'),
    field_decl: $ => seq(
      field('name', $.identifier),
      '{',
      'type',
      ':',
      field('type', $.type),
      '}',
    ),

    invariants_block: $ => seq('invariants', '{', repeat($.invariant_decl), '}'),
    invariant_decl: $ => seq(
      field('name', $.identifier),
      '{',
      repeat($._invariant_clause),
      '}',
    ),
    _invariant_clause: $ => choice(
      seq('when', field('when', $.block_expr)),
      seq('require', field('require', $.block_expr)),
    ),
    block_expr: $ => seq('{', $._expression, '}'),

    semantic_identity_block: $ => seq('semantic_identity', '{', repeat($._identity_entry), '}'),
    _identity_entry: $ => choice(
      seq(field('key', choice('core_capability', 'forbidden_drift')), ':', field('value', $.bracket_string_list)),
      seq(field('key', 'drift_tolerance'), ':', field('value', $.number)),
    ),
    number: $ => token(/-?[0-9]+(\.[0-9]+)?/),
    bracket_string_list: $ => seq('[', optional(seq($.string, repeat(seq(',', $.string)), optional(','))), ']'),

    evolution_block: $ => seq('evolution', '{', repeat($._evolution_entry), '}'),
    _evolution_entry: $ => seq(
      field('key', choice('allowed', 'forbidden', 'requires_gate')),
      ':',
      field('value', $.bracket_string_list),
    ),

    // ============ state ============
    state_def: $ => seq(
      'state',
      field('name', $.identifier),
      '{',
      repeat($.state_value),
      '}',
    ),
    state_value: $ => seq(
      'value',
      field('name', $.identifier),
      '{',
      repeat($.assist_field),
      '}',
    ),

    // ============ transition ============
    transition_def: $ => seq(
      'transition',
      field('name', $.identifier),
      '{',
      repeat($._callable_member),
      '}',
    ),

    // ============ error ============
    error_def: $ => seq(
      'error',
      field('name', $.identifier),
      '{',
      repeat($.error_variant),
      '}',
    ),
    error_variant: $ => seq(
      'variant',
      field('name', $.identifier),
      '{',
      optional(seq($.variant_field, repeat(seq(';', $.variant_field)), optional(';'))),
      '}',
    ),
    variant_field: $ => seq(
      field('name', $.identifier),
      ':',
      field('type', $.type),
    ),

    // ============ capability ============
    capability_def: $ => seq(
      'capability',
      field('name', $.identifier),
      '{',
      repeat(choice($.allow_block, $.deny_block)),
      '}',
    ),
    allow_block: $ => seq('allow', '{', repeat($._effect_stmt), '}'),
    deny_block: $ => seq('deny', '{', repeat($._effect_stmt), '}'),
    _effect_stmt: $ => seq($.effect_ref, optional(';')),
    // effect 引用：`Pure` 或 `Family.Op` / `Family.Op(args)`（统一形态，不再硬编码
    // Console.Write / DB.Read 等）。family/op 为标识符；实参为字面量或绑定名。
    effect_ref: $ => choice(
      'Pure',
      seq(
        field('family', $.identifier),
        '.',
        field('op', $.identifier),
        optional(seq('(', optional(seq($._effect_arg, repeat(seq(',', $._effect_arg)))), ')')),
      ),
    ),
    _effect_arg: $ => choice($.string, $.int, $.bool, $.identifier),

    // ============ effect 声明（内置/领域 effect 族） ============
    effect_def: $ => seq(
      'effect',
      field('name', $.identifier),
      '{',
      repeat(choice($.assist_field, $.effect_operation)),
      '}',
    ),
    effect_operation: $ => seq(
      'operation',
      field('name', $.identifier),
      optional(seq('{', repeat($.effect_param), '}')),
    ),
    effect_param: $ => seq(
      'param',
      field('name', $.identifier),
      ':',
      field('type', $.type),
      optional(';'),
    ),

    // ============ storage ============（已移除：storage 节点语义不清，I/O 改由标准库提供，
    // 见 docs/stdlib_design.md）

    // ============ action ============
    action_def: $ => seq(
      'action',
      field('name', $.identifier),
      '{',
      repeat($._callable_member),
      '}',
    ),

    // transition / action 共享的成员
    _callable_member: $ => choice(
      $.assist_field,
      $.capability_binding,
      $.intent_conversion_flag,
      $.input_block,
      $.output_block,
      $.effects_block,
      $.errors_block,
      $.body_block,
      $.requires_block,
      $.ensures_block,
    ),
    capability_binding: $ => seq('capability', ':', field('name', $.identifier)),
    intent_conversion_flag: $ => seq('intent_conversion', ':', field('value', $.bool)),

    input_block: $ => seq('input', '{', optional($.param_list), '}'),
    output_block: $ => seq('output', '{', optional($.param_list), '}'),
    param_list: $ => seq($.param_decl, repeat(seq(';', $.param_decl)), optional(';')),
    param_decl: $ => seq(
      field('name', $.identifier),
      ':',
      field('type', $.type),
      optional(seq('where', field('predicate', $._expression))),
    ),

    effects_block: $ => seq('effects', '{', repeat($._effect_stmt), '}'),
    errors_block: $ => seq('errors', '{', optional(seq($.identifier, repeat(seq(';', $.identifier)), optional(';'))), '}'),
    requires_block: $ => seq('requires', '{', repeat($._expression), '}'),
    ensures_block: $ => seq('ensures', '{', repeat($._expression), '}'),

    // ============ task ============
    task_def: $ => seq(
      'task',
      field('name', $.identifier),
      '{',
      repeat($._task_member),
      '}',
    ),
    _task_member: $ => choice(
      seq(field('key', 'goal'), ':', field('value', $.string)),
      $.include_block,
      $.exclude_block,
    ),
    include_block: $ => seq('include', '{', repeat($.include_decl), '}'),
    include_decl: $ => seq(
      field('kind', choice('entity', 'state', 'error', 'capability', 'transition', 'action')),
      field('name', $.identifier),
      optional(';'),
    ),
    exclude_block: $ => seq('exclude', '{', repeat($._effect_stmt), '}'),

    // ============ body 子语言 ============
    body_block: $ => seq('body', $.block),
    block: $ => seq('{', repeat($._statement), '}'),

    _statement: $ => choice(
      $.let_stmt,
      $.set_stmt,
      $.return_stmt,
      $.raise_stmt,
      $.if_stmt,
      $.match_stmt,
      $.repeat_stmt,
      $.print_stmt,
      $.expression_stmt,
    ),

    let_stmt: $ => seq(
      'let',
      optional('mutable'),
      field('name', $.identifier),
      '=',
      field('value', $._expression),
    ),
    set_stmt: $ => seq('set', field('name', $.identifier), '=', field('value', $._expression)),
    return_stmt: $ => seq('return', field('value', $._expression)),
    raise_stmt: $ => seq('raise', field('value', $.entity_construction)),
    print_stmt: $ => seq('print', field('value', $._expression)),
    expression_stmt: $ => $._expression,

    if_stmt: $ => prec.right(seq(
      'if',
      field('condition', $._no_struct_expr),
      field('consequence', $.block),
      optional(seq('else', field('alternative', choice($.block, $.if_stmt)))),
    )),

    match_stmt: $ => seq(
      'match',
      field('subject', $._no_struct_expr),
      '{',
      repeat($.match_arm),
      '}',
    ),
    match_arm: $ => seq(
      field('pattern', $.pattern),
      '=>',
      field('body', choice($.block, $._statement)),
    ),
    // match pattern。永久禁止 `_` catch-all（语法层不可解析）。
    // 见 docs/type_system.md §三：one of 的成员按 tag 分派。
    pattern: $ => choice(
      $.bool,                                             // Bool 主语：true / false
      $.qualified_name,                                   // state 值：TodoStatus.Done
      'Null',                                             // Null 成员
      $.type_pattern,                                     // 类型 pattern：`Int x` / `Todo t`
      $.variant_pattern,                                  // error variant：`V { f, ... }`
    ),
    // 类型 pattern：匹配 one of 的标量 / entity / state 成员并绑定。`<类型名> <绑定名>`。
    type_pattern: $ => seq(
      field('ty', $.identifier),
      field('binding', $.identifier),
    ),
    // variant pattern：匹配 error variant 成员，按字段名绑定。`V { f1, f2 }`。
    variant_pattern: $ => seq(
      field('variant', $.identifier),
      optional(seq(
        '{',
        optional(seq($.identifier, repeat(seq(',', $.identifier)), optional(','))),
        '}',
      )),
    ),

    repeat_stmt: $ => seq(
      'repeat',
      field('count', $._no_struct_expr),
      'times',
      field('body', $.block),
    ),

    // ---- 表达式 ----
    // 点号访问统一用 field_access / method_call 表达；HIR 层根据 head 是否解析为
    // state/error 类型来区分 “状态值访问” 与 “字段访问”。qualified_name 仅用于 pattern。
    _expression: $ => choice(
      $.entity_construction,
      $._no_struct_expr,
    ),
    // 在 if / match / repeat 的头部位置，裸 entity_construction 会与 “标识符 + 块体”
    // 产生歧义（典型 struct-literal-in-condition 问题）。这些位置使用不含裸构造的表达式；
    // 如需构造，可加括号。
    _no_struct_expr: $ => choice(
      $.string,
      $.int,
      $.bool,
      'Null',
      $.list_literal,
      $.call_expr,
      $.method_call,
      $.field_access,
      $.identifier,
      $.unary_expr,
      $.binary_expr,
      $.paren_expr,
    ),
    paren_expr: $ => seq('(', $._expression, ')'),
    list_literal: $ => seq('[', optional(seq($._expression, repeat(seq(',', $._expression)), optional(','))), ']'),
    qualified_name: $ => seq(field('head', $.identifier), '.', field('value', $.identifier)),
    field_access: $ => prec.left(8, seq(field('base', $._no_struct_expr), '.', field('field', $.identifier))),

    entity_construction: $ => prec(1, seq(
      field('name', $.identifier),
      '{',
      optional(seq($.field_assign, repeat(seq(optional(choice(',', ';')), $.field_assign)), optional(choice(',', ';')))),
      '}',
    )),
    field_assign: $ => seq(field('name', $.identifier), '=', field('value', $._expression)),

    call_expr: $ => prec(4, seq(
      field('callee', $.identifier),
      '(',
      optional(seq($._expression, repeat(seq(',', $._expression)))),
      ')',
    )),
    method_call: $ => prec.left(9, seq(
      field('base', $._no_struct_expr),
      '.',
      field('method', $.identifier),
      '(',
      optional(seq($._expression, repeat(seq(',', $._expression)))),
      ')',
    )),

    unary_expr: $ => prec(7, seq(field('op', choice('not', '-')), $._no_struct_expr)),
    binary_expr: $ => {
      const table = [
        ['or', 1],
        ['and', 2],
        ['==', 3], ['!=', 3], ['<', 3], ['<=', 3], ['>', 3], ['>=', 3],
        ['+', 4], ['-', 4],
        ['*', 5],
      ];
      return choice(...table.map(([op, p]) => prec.left(p, seq(
        field('left', $._no_struct_expr),
        field('op', op),
        field('right', $._no_struct_expr),
      ))));
    },
  },
});
