## 变更说明

<!-- 做了什么，解决什么问题 -->

## 动机 / 设计

<!-- 为什么这样做；涉及的设计决策（如有，链接 docs/engineering_notes.md 条目） -->

## 验证

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] 已加 / 更新回归测试
- [ ] 已同步文档（`docs/dev_checklist_v1.md` 变更记录；必要时 `engineering_notes.md` / `workflow_graph_spec.md`）

## 检查清单（见 CONTRIBUTING.md）

- [ ] 遵循单一路线（无多路径 / 双栈 / 功能性 fallback）
- [ ] 遵循分层纪律（`core/*` 零 IO、不依赖 `workflow/*`；编译器不调用 LLM）
- [ ] 无临时 / 调试代码，无密钥入库
- [ ] 注释与文档为中文
