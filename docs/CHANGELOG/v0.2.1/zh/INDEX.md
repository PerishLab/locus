# Locus 0.2.1

Locus CLI 现在通过 `locus query <ROLE> [KEY]` 提供精确、只读的 Atom
检索。只有 role 时按序列出 identity 与 record count；增加 key 后则按输入
顺序回放匹配的逻辑 Atom。

每次完整 query 都以 `locus.query/v1` summary 收尾，明确报告输入、匹配与
identity coverage。CLI 会在输出前验证完整输入，因此 malformed JSONL 会
直接拒绝而不产生 partial result。Query 保持 stdin-first，不要求产品根或
analyzer declaration。

在 Unix 上，file reporter 现在以 owner-only `0600` 权限创建新 report。
已有 report 的权限不会被修改。临时 Locus skill 记录了产品在零源码侵入
delivery 出现前应遵守的单向、default-muted adapter 约定。
