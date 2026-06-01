//! WASM 值 ABI 与函数 ABI 常量（见 docs/wasm_codegen.md §四 / §五）。
//!
//! **值 ABI**：所有 Sophia 值在线性内存的 bump 区分配，统一表示为 `[tag:i32][payload...]`；
//! WASM 栈 / 局部 / 参数 / 返回值一律传 **i32 句柄**（指向值在内存中的偏移）。tag 与
//! `runtime::Value` 变体一一对应。**intent 运行时擦除**——值表示不带 intent 标签（与解释器一致）。
//!
//! **函数 ABI**：每个 callable 编译为一个 WASM function，签名 `(i32 句柄 × N) -> i32`（返回
//! **Outcome 句柄**）。Outcome = `[kind:i32][value_handle:i32]`，`kind` 0=Returned / 1=Raised
//! ——复刻解释器的 `Outcome`，raise 经返回通道冒泡（不用 WASM 异常扩展，见 §四决策点 ④）。
//!
//! W2 覆盖的值：`Unit` / `Bool` / `Int` / `Null` / `ErrorValue`（标量 + 错误返回成员）。
//! `Text` / `List` / `Entity` / `State` 的值布局在后续增量落地（W2b 起）。

/// 值标签（与 `runtime::Value` 变体一一对应；与设计 §四一致）。
///
/// 这是 codegen 与解释器之间**规范的值标签表**——编号固定、完整列出（类比 exec-ir `EdgeKind`
/// 保留完整词汇表）。W2 仅产出 `UNIT`/`BOOL`/`INT`/`NULL`；`TEXT`/`LIST`/`ERROR_VALUE`/`ENTITY`/
/// `STATE` 随各自增量启用，故 `allow(dead_code)` 保留为完整表、非投机代码。
#[allow(dead_code)]
pub mod tag {
    pub const UNIT: i32 = 0;
    pub const BOOL: i32 = 1;
    pub const INT: i32 = 2;
    pub const TEXT: i32 = 3; // W2b
    pub const NULL: i32 = 4;
    pub const LIST: i32 = 5; // W2b
    pub const ERROR_VALUE: i32 = 6; // W2b
    pub const ENTITY: i32 = 7; // W2b
    pub const STATE: i32 = 8; // W2b
}

/// Outcome 的 kind 判别（函数返回值）。
pub mod outcome {
    /// 正常 `return`。
    pub const RETURNED: i32 = 0;
    /// `raise` 的领域错误。
    pub const RAISED: i32 = 1;
}

// ---- 值的内存布局（字节偏移，小端；WASM 线性内存天然小端）----
//
// 所有偏移 / 大小以字节计。i64 payload 放在 +8（8 字节对齐，干净且确定）。

/// 标签字段偏移（所有值首字段）。
pub const OFF_TAG: u64 = 0;

/// `Int` 值：`[tag:i32@0][i64@8]`，大小 16。
pub const OFF_INT_PAYLOAD: u64 = 8;
pub const SIZE_INT: i32 = 16;

/// `Bool` 值：`[tag:i32@0][i32@4]`，大小 8。
pub const OFF_BOOL_PAYLOAD: u64 = 4;
pub const SIZE_BOOL: i32 = 8;

/// `Text` 值：`[tag:i32@0][bytes_ptr:i32@4][byte_len:i32@8]`，大小 12。
/// bytes 指向常量字符串区（字面量）或 bump 堆（拼接 / `to_text` 结果）。
pub const OFF_TEXT_PTR: u64 = 4;
pub const OFF_TEXT_LEN: u64 = 8;
pub const SIZE_TEXT: i32 = 12;

/// `Null` / `Unit` 值：仅 `[tag:i32@0]`，大小 4。
pub const SIZE_TAGONLY: i32 = 4;

// ---- 具名记录（`ErrorValue` 复用；W2c 起 `Entity` 同布局）----
//
// `[tag@0][name_ptr:i32@4][name_len:i32@8][nfields:i32@12]`，再接 `nfields` 个字段，每个
// `[key_ptr:i32@0][key_len:i32@4][val_handle:i32@8]`（12 字节）。字段按 key 字典序存放（与解释器
// `BTreeMap` 一致），保证 emit 确定 + 结构相等可逐位比较。

/// 记录名指针 / 长度（`ErrorValue` 的 variant 名；`Entity` 的 entity 名）。
pub const OFF_REC_NAME_PTR: u64 = 4;
pub const OFF_REC_NAME_LEN: u64 = 8;
pub const OFF_REC_NFIELDS: u64 = 12;
pub const REC_HEADER_SIZE: i32 = 16;
/// 单字段大小与子偏移（相对字段基址）。
pub const REC_FIELD_SIZE: i32 = 12;
pub const OFF_FIELD_KEY_PTR: u64 = 0;
pub const OFF_FIELD_KEY_LEN: u64 = 4;
pub const OFF_FIELD_VAL: u64 = 8;

// ---- `State` 值：`[tag@0][state_ptr:i32@4][state_len:i32@8][value_ptr:i32@12][value_len:i32@16]` ----
// 大小 20（对齐到 24）。state / value 名均指向常量字符串区。
pub const OFF_STATE_NAME_PTR: u64 = 4;
pub const OFF_STATE_NAME_LEN: u64 = 8;
pub const OFF_STATE_VALUE_PTR: u64 = 12;
pub const OFF_STATE_VALUE_LEN: u64 = 16;
pub const SIZE_STATE: i32 = 20;

/// Outcome：`[kind:i32@0][value_handle:i32@4]`，大小 8。
pub const OFF_OUTCOME_VALUE: u64 = 4;
pub const SIZE_OUTCOME: i32 = 8;

/// bump 分配对齐（8 字节，保证 i64 payload 自然对齐）。
pub const ALLOC_ALIGN: i32 = 8;
