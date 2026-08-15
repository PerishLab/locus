# Locus 0.2.2

Locus CLI 新增 `locus span` 派生 span 时间。它把一条 entering source 记录与其
returning 记录配对，报出每个被追踪声明的 elapsed 与 held 时间，并以
`locus.span/v1` summary 收尾，覆盖输入、匹配记录、声明数与未闭合 span 数。

held 时间把「同一 trace 内的包含关系」当作记录所携带的**唯一**嵌套证据。若 span
彼此重叠而非嵌套，则整体拒绝该派生，而不是去分摊一段记录本身无法归属的时长 ——
并发席位得到的是显式拒绝，而不是一个悄悄算错的数。

已经 enter 却从未 return 的 span 会被具名报出，而不是丢弃。panic、abort、
cancellation 与进程丢失都留下这个形状，而它死在哪个声明里正是读者要的。

底座未变。parent role、lifecycle、attribution 与产品词汇都没有进入 Atom 流；
每一个派生量都留在 acceptance 的下游，并保持 stdin-first。
