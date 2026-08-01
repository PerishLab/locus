# 迁移

现有 Rust API consumer 不需要源码迁移。`locus`、`locus-macro` 与
`locus-cli` crate version 继续作为同一个 coupled release 前进。

CLI consumer 可以按需采用新的 query 命令：

```sh
locus query locus.trace < atoms.jsonl
locus query locus.trace trace-key < atoms.jsonl
```

新建 Unix report file 现在仅 owner 可访问。若 operator 确实需要与 group
共享 report，应在创建后显式施加该策略，或提供一个具有目标权限的既有
endpoint。Locus 不会修改既有 report file 的 mode。
