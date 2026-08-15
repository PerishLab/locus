# 迁移到 Locus 0.3.0

Locus 0.3.0 保留 0.2.2 的底座。Context、Atom、collector、generator、reporter、hook
与 query 的行为均未改变，现有报告无需转换即可读取。

只做接受与上报的产品什么都不欠。库与宏随发布携带版本号，接口未变；
钉住 `=0.2.2` 的消费方可以按自己的节奏推进。

**三处消费面发生了变化。读取 `locus span` 或 `locus inspect` 输出的一方需要更新。**

## `locus span` 遇到交错不再以 2 退出

在 0.2.2 里，span 部分重叠而不嵌套的流以退出码 2 拒绝且不产生任何输出。
现在它以 0 退出并给出派生，每个交错窗口报告为一条记录：

```json
{"schema":"locus.span/v1","kind":"tangled","trace":"...","at":0,"until":30,
 "spans":2,"declarations":["m::inner","m::outer"]}
```

**把非零退出当作「这份流里有交错」的调用方必须改读 summary**：

```sh
locus span < atoms.jsonl | tail -1
```

`refused` 为零，当且仅当没有 span 被丢弃。把非零退出当作「不会有输出」的调用方
必须停止丢弃 stdout。

退出码 2 仍然表示拒绝，保留它的那几种拒绝未变：span 未进入即返回、从另一个 declaration 返回、
或返回早于进入。**那些拒的是输入本身，而不是输入的某个区域。**

## `locus.span/v1` summary 增加两个字段

summary 新增 `tangled`（交错窗口数）与 `refused`（窗口内被丢弃的 span 数）。
对 summary 对象做过精确形状断言的读取方需要放宽断言。此前发出的每个字段都保留名字与含义。

## `locus.inspect/v1` summary 增加 `refused`

inspection summary 新增 `refused`，即 held-time mapping 丢弃的 span 数。
未声明 held-time analyzer 的配置该值恒为零，所以现有 `locus.toml` 只会看到多出一个字段。

## `held-time-nanoseconds` 是新增的 mapping 种类

声明它是可选的，现有配置不受影响。阈值是按 Context 分组的 held 时间纳秒下界：

```toml
[[analyzer]]
id = "trace.held"
mapping = { kind = "held-time-nanoseconds", group = "locus.trace" }
threshold = { above = 1000000000 }
```

分组键读自进入记录自己的 Context。source 记录上不携带产品 role 的产品只能按 `locus.trace` 分组；
在那些记录上绑定 role 是该产品在自己接受面上的决定，**不是本次发布所欠**。
