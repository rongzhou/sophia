# 贡献指南

感谢有意参与 Sophia。本文档说明开发流程与代码规范。构建与测试见 [INSTALL.md](INSTALL.md)。

## 核心原则

- **单一路线**：任何层面不允许多路径 / 双栈 / 向后兼容负担 / 功能性 fallback。设计变更直接迁移、移除旧路径。占位须位于唯一代码路径内、清晰返回未实现错误，而非伪造 fallback。
- **诚实性**：绝不伪造成功；硬错误如实阻断（"待接入"诚实标注）。
- **分层纪律**：`core/*` 零 IO、不依赖 `workflow/*`；`tools/*` 确定性、不依赖工作流图；编译器不调用 LLM。
- **注释与文档统一中文**，英文术语采用「中文（英文术语）」形式首次出现时注明。

详见 `docs/engineering_notes.md`（决策日志，含上述原则的完整表述）。

## 每次改动的流程

1. **先读后写**：改动前读相关代码与设计文档（`docs/`），理解设计意图。
2. **实现**：以最理想设计落地，不打补丁、不折中。
3. **加回归测试**：新功能 / 修 bug 都应带测试。
4. **同步文档**：更新 `docs/dev_checklist_v1.md`（当前进展 SSOT + 变更记录）；涉及决策时在 `docs/engineering_notes.md` 新增条目；涉及图 schema 时更新 `docs/workflow_graph_spec.md`。
5. **验证**（必须全绿）：

   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

## 提交规范

- 提交信息用中文，首行简明概括，正文说明「做了什么 / 为什么 / 验证状态」。
- 仅在改动逻辑完整、测试通过后提交；避免围绕同一文件的碎片化小步反复提交。
- 不提交临时 / 调试代码与生成产物（`target/`、`sophia-runs/` 生成物、`*.sqlite` 等已在 `.gitignore`）。
- 不提交密钥：API key 仅经环境变量（如 `SOPHIA_LLM_API_KEY`）读取，不得落盘或入库。

## 测试约定

- 单元 / 集成测试是确定性的，进 `cargo test` 门禁。
- 真实 LLM 的端到端测试是 `example`，**不进** CI，按需手动运行（见 `docs/e2e_test_design.md`）。
- 快照测试用 [`insta`](https://insta.rs/)：新增 / 变更后用 `cargo insta review` 审阅并接受 `.snap.new`。
- 防答案泄漏是 e2e 的第一原则：任务答案不得进入共享脚手架（语法基线 / system prompt）。

## 修改语法

改动 `core/syntax/grammar.js` 须用对齐版本的 Tree-sitter CLI 重新生成 `parser.c`（见 INSTALL.md「修改语法」）。版本对齐（crate / CLI / ABI 三者一致）是硬约束。

## 许可

提交贡献即表示同意以本项目的 MIT License 授权你的贡献，无附加条款。
