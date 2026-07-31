# 迁移

`locus inspect` 现在要求一个包含 `locus.toml` 的产品根。复制或调整 Locus
仓库根的声明，然后把产品根作为可选位置参数传入：

```sh
locus inspect /path/to/product < atoms.jsonl
```

stdout consumer 必须接受 `locus.inspect/v1` record。Finding 现在明确携带
analyzer、group、measurement 与 threshold；每次成功 inspection 都以
summary record 结束。因此 clean inspection 不再意味着 stdout 为空。

`locus` 与 `locus-macro` Rust API 不需要源码迁移；它们的版本随 coupled
release 一同前进。
