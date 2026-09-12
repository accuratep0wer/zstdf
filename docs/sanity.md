# CP/FT source sanity

`sanity` reads STDF files, directories and gzip inputs directly. It checks every
record, displays important run metadata, and offers compact per-unit previews.
It does not change Dashboard yield or the `traceability` command.

## 安装与运行

在仓库根目录构建：

```powershell
cargo build -p stdf-cli --release --locked
.\target\release\zstdf-cli.exe sanity --help
```

单一 CP / FT 使用 `--test-domain`，混合输入使用 `--run-profiles`。省略 `--profile` 时使用内置基础 profile；示例 profile 的命名格式
仅用于合成数据，请按实际产品修改。

```powershell
.\target\release\zstdf-cli.exe sanity C:\data\cp `
  --test-domain cp --profile examples\sanity\cp-profile.json `
  --output-dir C:\reports\cp-sanity

.\target\release\zstdf-cli.exe sanity C:\data\ft.stdf.gz `
  --test-domain ft --profile examples\sanity\ft-profile.json `
  --output-dir C:\reports\ft-sanity

Start-Process C:\reports\cp-sanity\report.html
```

Linux/macOS 使用 `target/release/zstdf-cli`，命令参数相同。

## 可运行合成示例

```powershell
python examples\sanity\generate_demo.py
.\target\release\zstdf-cli.exe sanity examples\sanity\generated\cp.stdf `
  --test-domain cp --profile examples\sanity\cp-profile.json `
  --output-dir examples\sanity\generated\cp-report
.\target\release\zstdf-cli.exe sanity examples\sanity\generated\ft.stdf.gz `
  --test-domain ft --profile examples\sanity\ft-profile.json `
  --output-dir examples\sanity\generated\ft-report
Start-Process examples\sanity\generated\cp-report\report.html
```

`ft-invalid.stdf` 在第三个 unit 的第三条 PTR 中含 NaN。它不在前两条预览内，
但全文件检查应报告错误并返回退出码 1。这用于验证预览不会遮蔽后部异常。

## 报告内容

- **Per run**：FAR 的 CPU_TYPE/STDF_VER；MIR 中的 lot/sublot、产品、程序/版本、步骤、
  操作员、温度、时间、STAT_NUM、MODE_COD、RTST_COD、DATE_COD、厂区/楼层/工艺；SDR 的
  head/site、SITE_CNT 与硬件/EXTR_ID；适用的 WIR/WRR/WCR、SBR/HBR、MRR/PCR。
  保留同一 run 内每次上下文记录。
- **Per unit**：每次测试独立列出 PART_ID、wafer、坐标合并键和 PRR 判定/bin。
  按记录类型各取前两条：DTR 的文本、PTR.RESULT、MPR.RTN_RSLT[0]、FTR 有效判定、
  GDR 第一个非填充元素。零/一条不补齐，无效首值不跳过。
- **全文件诊断**：字段边界、数组长度、非有限数值、字符编码、部分缺失/有效性规则、
  CP wafer 关联、run/unit/wafer/程序段闭合、配置的产品格式。多 site 交错独立跟踪。
- **离线交互**：run 选择、unit 搜索/分页/选择、诊断筛选、完整摘要 JSON 导出。

首值直接展示数值及已知单位，FTR 显示 PASS/FAIL/Unknown；浮点 bits、来源偏移与
继承信息收纳在可展开的 **Raw evidence** 中。R*8 显示保留原始十进制字符串精度，
不会先转换为 JavaScript Number 再四舍五入。导出的 JSON 和 Parquet 证据不受显示格式影响。
run 下拉框显示 CP/FT 及 profile ID；存在 warning 时顶部明确提示待检查的 warning 数量。

字段的 `explicit` 仅表示文件写入了该值，不能证明是人工输入。
`standard_default`、`inherited`、`unresolved` 与值的有效性分别保存。
MPR 首值旁的状态属于整个结果字段；后续元素无效时，字段状态也会提示异常。
DTR/GDR 仅在一个 unit 明确打开时关联到该 unit；多 site 不明确的记录留在 run 层。

## 证据与发布

输出目录下 `report.html` 是唯一发布入口；它固定引用一个不可变的 `sanity-*` generation。
每个 generation 包含：

| 文件 | 内容 |
| --- | --- |
| `summary.json` | 完整 run/unit 摘要、诊断、profile/hash、扫描完整性 |
| `record_fields.parquet` | 每字段一行；source、record offset/type、字段名/类型、字节范围及 `evidence_json` |
| `<sha256>.stdf` | 解压后的原始证据；相同内容只保留一次，摘要保留全部输入路径 |
| `manifest.json` | 同 generation 文件的 SHA-256 和大小 |
| `report.html` | 该 generation 的离线报告副本 |

Parquet 的 `byte_start` 相对**记录头开始**；加 `record_offset` 就是原始文件位置。
`evidence_json` 保存原始/有效值、字段状态和来源；数组保留所有元素，R*4/R*8 保存
十六进制 raw bits，避免 JSON/JavaScript 的数值表示损失。乱码的精确字节从原始证据读取，
不能把解码后的替换字符当原始字节。Header 字段也在证据表中。

成功完成诊断但发现数据问题时，发布带失败状态的报告，退出码为 1。
损坏输入可生成 `scan_complete=false` 的诊断报告；后部未扫描部分不声称通过。
文件读取/写入失败、超限、取消不替换旧报告。旧 generation 保留，便于审查之前结果；
磁盘预算只约束本次 generation 和原子发布副本，不包括以前已发布的 generation。

## Profile 与资源参数

Profile 使用严格 JSON schema：`version: 1`、非空 `id`、`domain: cp|ft`，可配置
`require_wafer`、`rules`、`important_fields`。拼错的字段、未知参数、错误正则会被拒绝。
每条规则包含 `record`、`field` 和可选的 `required`、`pattern`、`allowed`、`min`、`max`。
`pattern` 匹配完整字符串；范围规则作用于有效数值。规则针对实际出现的记录逐条执行，
不是要求任意记录类型必须出现。run/MIR/MRR、unit 闭合和 CP wafer 要求由结构规则负责。

例如，只展示 MIR 的几个字段（不改变全字段检查/导出）：

```json
{
  "version": 1,
  "id": "my-ft-v1",
  "domain": "ft",
  "important_fields": {"MIR": ["LOT_ID", "JOB_NAM", "JOB_REV", "TST_TEMP"]},
  "rules": [{"record": "MIR", "field": "JOB_REV", "required": true}]
}
```

| 参数 | 默认 | 作用 |
| --- | --- | --- |
| `--preview-records-per-type` | 2 | 每个 unit、每类记录的预览条数；范围 1–100 |
| `--max-report-mib` | 32 | 摘要保留预算及序列化报告上限，不是进程 RSS 硬限制 |
| `--disk-limit-mib` | 1024 | 本次原始证据、Parquet、摘要和发布副本的写入预算 |
| `--max-units` | 100000 | 测试实例数量上限；超限失败，不截断 |
| `--max-sources` | 10000 | 输入发现上限，在内容去重前执行 |
| `--cancel-file` | 无 | 文件出现即协作取消，发布前再次检查 |

## 混合 CP / FT 和每次运行的配置

```powershell
.\target\release\zstdf-cli.exe sanity `
  examples\sanity\generated\cp.stdf examples\sanity\generated\ft.stdf `
  --run-profiles examples\sanity\run-profiles.json `
  --output-dir examples\sanity\generated\mixed-report
```

`--run-profiles` 与 `--test-domain`、`--profile` 互斥。配置包含 `version: 1`、`id` 和
`routes`；每条 route 的 `profile` 是完整的内嵌 CP/FT profile，`matches` 是候选条件列表。
同一条件中所有字段为 AND，多组条件为 OR；每个 MIR 必须恰好匹配一条 route。
示例使用 MIR.JOB_NAM 和 JOB_REV 精确匹配，不根据文件名或 WIR 存在与否猜测 CP/FT。

条件可包含 `mir` 字段值对象、`source_id`（解压后 SHA-256，小写十六进制）和
`mir_offset`（解压后 MIR 记录偏移的十进制**字符串**）。每个 run 保存实际 profile ID/hash；
整个报告也保存路由配置及其 hash。未匹配或多重匹配的 run 保留原始证据和 units，
但 domain 显示 unknown、产品规则不执行，并以非零退出码发布明确失败的诊断报告。
配置格式错误则在发布前拒绝，保留原报告。

## 当前覆盖边界

这是可运行的 **10D.4 首版**，不是完整标准符合性认证，也未完成 10D.5 的存储优化。
目前实现基础 v4 的 25 类记录字段布局、原始证据、重要字段预览和上述规则。
未知/vendor/v4-2007 记录保留原始字节，并将本次检查标为失败/不支持。
尚未实现所有标准枚举/汇总计数对账、FTR 定义继承、完整规则覆盖清单，以及大规模 RSS/故障恢复验收。
PTR/MPR 默认定义按 source + MIR run + record type + TEST_NUM 隔离，使用第一次定义；
后续覆盖只影响当前记录。支持尺度、限制、MPR 输入参数/索引、单位和显示格式，并保留来源。
无效或缺少的首条定义保持 unknown；显式无上/下限优先于默认请求；单字节 NUL 字符串可显式清空默认文本。
不执行测试的首次定义记录可保留在 run 层，不计为 unit 的测试记录。

当前字段值保存在 Parquet 的 JSON 证据列及原始 STDF 中，使用有界批次、Snappy 和关闭的
字典编码。现有 EAV 列类型未变化；这不代表已经完成逐列压缩基准或得到空间改善。

## 验证

```powershell
cargo fmt --all --check
cargo test --workspace --exclude stdf-py --locked --offline
cargo check -p stdf-py --locked --offline
cargo build -p stdf-cli --release --locked --offline
# 本地已有 Playwright 和 Edge 时：
node scripts\smoke_sanity.cjs examples\sanity\generated\cp-report\report.html
```

规则参考：[Teradyne STDF v4 specification](https://storage.googleapis.com/google-code-archive-downloads/v2/code.google.com/stdf-eclipse/Stdf-V4-spec.pdf)。
完整的后续交付要求见 [Phase 10D.4–10D.5](../PHASED_EXECUTION_SPEC.md)。
