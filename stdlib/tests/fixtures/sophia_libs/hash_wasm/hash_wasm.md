三方库形态演示 · `hash_wasm`（整数 digest，WASM host 实现）。仅供库插件机制演示。

## 用途

提供一个确定的整数 digest 操作 `WasmHash.Mix(seed, value)`——与 `hash_sophia` 计算相同，但经
**WASM host**（surface = effect-op，host = WASM）实现，演示三方 WASM 库的统一加载链路。

## 操作

- `WasmHash.Mix(seed, value)`：输入两个整数，输出整数 digest（同 `SophiaDigest`）。纯计算
  （`effectful = false`），调用形态为特殊根 `WasmHash.Mix(s, v)`，无需 effect / capability 声明。

> host 由库目录下的 `host.wasm` 提供（导出 `memory`、`sophia_alloc`、`sophia_read_copy`、
> `wasm_hash_mix(args_ptr, args_len) -> result_len`，统一 ValueWire ABI）。
