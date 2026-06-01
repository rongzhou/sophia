标准库形态演示 · `hash_sophia`（整数 digest，纯 Sophia 实现）。仅供库插件机制演示。

## 用途

提供一个确定的整数 digest 助手 action `SophiaDigest(seed, value)`——纯逻辑、无副作用，演示
**纯 Sophia 源码库**（surface = Sophia 源码节点，host = none）。

## 操作

- `SophiaDigest(seed, value)`：输入两个整数，输出整数 digest（`acc = acc*31 + value` 重复 3 次，
  初值 `acc = seed`）。

无 effect、无 capability（纯计算 action，与用户自定义 action 同形）。
