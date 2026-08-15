# 迁移到 Locus 0.2.2

Locus 0.2.2 保留 0.2.1 的底座。Context、Atom、collector、generator、reporter、
hook 与 query 行为均未改变，现有报告无需转换即可读取。

只做接受与上报的产品什么都不欠。库与宏随发布版本前进但接口未变；锁着
`=0.2.1` 的消费方可以按自己的节奏推进这个 pin。

`locus span` 是增量的。它不需要产品根，也不需要 analyzer 声明：

```sh
locus span < atoms.jsonl
```

若某条流里的 span 重叠而非嵌套，命令以退出码 2 拒绝且不产生部分输出。**那个拒绝
是预期结果，不是回归**：记录不携带 parent，held 时间本就不可派生，给一个分摊出来
的猜测比不给更糟。
