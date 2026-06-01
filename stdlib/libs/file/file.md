标准库 · File（本地文件读写）。仅当任务需要读取或写入本地文件时使用本库；否则忽略本节。

下列示例仅演示**库的用法形状**，与任何具体任务无关，请勿照抄其中的名字或逻辑。

## 用途

`File` 是读取 / 写入本地文件的标准库 effect 族。当任务需要"从某个路径读取文件内容"或"把内容写入某个
路径"时使用它。

## 操作

- `File.Read(path)`：读取给定路径的文件全文。
    - 参数 `path`：`Text` 类型（文件路径）。
    - 返回：`Raw<Text>`（文件内容，不可信外部数据）。
- `File.Write(path, content)`：把内容写入给定路径（覆盖）。
    - 参数 `path`：`Text` 类型（文件路径）。
    - 参数 `content`：`Sanitized<Text>` 类型（要写出的可信文本）。
    - 返回：`Unit`。

## effect 与 capability

`File.Read` / `File.Write` 是**副作用**。用到它们的 action 必须：

- 在 `effects { File.Read }` / `effects { File.Write }` 中声明用到的 effect；
- 用 `capability: <能力名>` 绑定一个 allow 了对应 effect 的 capability：
    capability <名字> {
      allow { File.Read; File.Write }
    }

未声明 effect 或未绑定具备该能力的 capability 都会被静态拒绝。

## intent 边界（务必遵守）

- `File.Read(path)` 取回的数据是 `Raw<Text>`——**不可信的外部原始数据**。它**不能**被直接当作可信值
  使用（不能直接 `print`、不能直接赋给要求 `Sanitized<T>` / `Validated<T>` 的字段或 output）。唯一合法
  路径：先经一个**显式 intent 转换 action**（`intent_conversion: true`，一入一出、同 inner 类型、不同
  intent、无 effect、body 直接 `return`）把 `Raw<Text>` 转成所需 intent，再使用。
- `File.Write(path, content)` 的 `content` 必须是 `Sanitized<Text>`——**不能**把未经处理的 `Raw<Text>`
  直接落盘（写出边界，同 `Console.Write`）。

## 中立语法形状示例（与任何任务无关，仅供参考形状，切勿照抄其名字或逻辑）

    capability ExampleFileCapability {
      allow { File.Read; File.Write }
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

    action ExampleLoad {
      capability: ExampleFileCapability
      input  { location: Text }
      output { content: Sanitized<Text> }
      effects { File.Read }
      body {
        let loaded = File.Read(location)
        let clean = ExampleToTrusted(loaded)
        return clean
      }
    }

    action ExampleStore {
      capability: ExampleFileCapability
      input  { location: Text; payload: Sanitized<Text> }
      output { done: Bool }
      effects { File.Write }
      body {
        File.Write(location, payload)
        return true
      }
    }
