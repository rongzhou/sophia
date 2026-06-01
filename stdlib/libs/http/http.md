标准库 · Http（网络获取）。仅当任务需要从网络取数据时使用本库；否则忽略本节。

下列示例仅演示**库的用法形状**，与任何具体任务无关，请勿照抄其中的名字或逻辑。

## 用途

`Http` 是发起网络请求、取回外部数据的标准库 effect 族。当任务需要"从某个 URL 获取内容"时使用它。

## 操作

- `Http.Get(url)`：对给定 URL 发起 GET 请求，取回响应体。
    - 参数 `url`：`Text` 类型（请求地址）。
    - 返回：`Raw<Text>`（响应体文本）。

## effect 与 capability

`Http.Get` 是一个**副作用**。用到它的 action 必须：

- 在 `effects { Http.Get }` 中声明该 effect；
- 用 `capability: <能力名>` 绑定一个 allow 了 `Http.Get` 的 capability：
    capability <名字> {
      allow { Http.Get }
    }

未声明 effect 或未绑定具备该能力的 capability 都会被静态拒绝。

## intent 边界（务必遵守）

`Http.Get(url)` 取回的数据是 `Raw<Text>`——**不可信的外部原始数据**。它**不能**被直接当作可信值使用：

- 不能直接 `print`（`Console.Write` 只接受字面量 / `Sanitized<T>` / `Redacted<T>`）；
- 不能直接赋给要求 `Sanitized<T>` / `Validated<T>` 等更强 intent 的字段或 output；
- 不能直接当作已校验数据继续处理。

唯一合法的使用路径：先经一个**显式 intent 转换 action**（`intent_conversion: true`，一入一出、同 inner
类型、不同 intent、无 effect、body 直接 `return`）把 `Raw<Text>` 转成所需 intent，再使用转换结果。

## 中立语法形状示例（与任何任务无关，仅供参考形状，切勿照抄其名字或逻辑）

    capability ExampleNetCapability {
      allow { Http.Get }
    }

    action ExampleToTrusted {
      intent_conversion: true
      input  { source: Raw<Text> }
      output { trusted: Sanitized<Text> }
      effects { Pure }
      body {
        return source
      }
    }

    action ExampleFetch {
      capability: ExampleNetCapability
      input  { endpoint: Text }
      output { content: Sanitized<Text> }
      effects { Http.Get }
      body {
        let fetched = Http.Get(endpoint)
        let clean = ExampleToTrusted(fetched)
        return clean
      }
    }
