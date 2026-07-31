# Locus 0.2.0

Locus inspection 不再把 analyzer 固化为编译期策略，而是把它们交还给产品声明。
根 `locus.toml` 将 Locus-owned mapping 与 threshold 组合，并把每个 analyzer
绑定到显式 Context role。

首批 mapping catalog 包含编码 representation bytes 与 dominant decoded
content-prefix percentage。Locus 根声明继续实例化既有的 8 KiB 大小粗筛和
80% 重复粗筛。

每次完整 inspection 现在输出 `locus.inspect/v1` JSONL finding，并以一条
summary 收尾；summary 明确列出 analyzer coverage、输入 record 数和 finding
数。空声明会报告零 coverage，不再暗示已经完成 clean analysis。

CLI 会在消费 stdin 前完整编译配置。未知字段、未知 mapping、非法 role、
重复 analyzer identity、malformed Atom 与 I/O failure 都会拒绝，且不输出
partial report。
