# zstdf 改进建议

面向手机阅读的摘要。标识符保持英文，方便对照代码。

## 摘要

对照了四类参考：

- [yieldHUB](https://www.yieldhub.com/platform)：STDF 自动入库、Bin Pareto、晶圆图叠图、参数分布、跨工序追溯。
- [Synopsys SiliconDash](https://www.synopsys.com/content/dam/synopsys/solutions/slm/datasheets/silicondashds.pdf)：一屏驾驶舱、晶圆图 / 趋势 / Pareto / 直方图 / CDF、规格尾部、近实时 Insights。
- [PDF Solutions Exensio](https://www.pdf.com/products/exensio-analytics-platform/modules/manufacturing-analytics/)：语义数据模型、良率问题排序、从总览钻到晶圆与测试项。2026 年的 [Exensio Aurora](https://www.pdf.com/resources/introducing-exensio-aurora-pdf-solutions-unveils-highly-scalable-architecture-for-exensio-analytics/) 把预聚合和按需取数分开。
- [Databricks 半导体湖仓](https://www.databricks.com/blog/semiconductors-data-intelligence-platform)：Bronze / Silver / Gold、列存、同一份数据服务报表和探索。Databricks 点名 STDF 需要专用解析器。

zstdf 已经有 STDF 解码、`eav-v2` Parquet、离线 HTML 报告、五套主题和五种语言。缺的是商用工具那种**打开就能看**的良率驾驶舱：Bin 损失排序、按晶粒上色的晶圆图、带规格线的参数分布，放在同一页。

**本次已实现：** 测量 Dashboard 的 Overview 改为良率驾驶舱。Hard / Soft Bin Pareto（柱 + 累积占比）、按合并后器件着色的晶圆图（可按晶圆筛选）、16 档参数直方图（有上下限时统计规格外尾部）、良率红绿灯。人口政策和原有页签没有改。

## 已实现（本轮）

| 建议 | 参考产品做什么 | 在 zstdf 里怎么落地 |
| --- | --- | --- |
| 一屏良率驾驶舱 | SiliconDash 的生产驾驶舱；yieldHUB 的 Bin + 晶圆 + 参数 | Overview 同时放 Bin Pareto、晶圆图、参数分布、Site 良率。`?lang=zh` 直接看中文。 |
| Bin Pareto | yieldHUB「先看最高失败 Bin」；Exensio 按影响排序 | 沿用现有 `hard_bins` / `soft_bins`。柱高是失败器件数；没有失败时退回器件数。折线是累积占比。 |
| 晶粒晶圆图 | SiliconDash / yieldHUB 的 bin map，而不是测试行密度 | 新增 `die_points`：每个有 X/Y 的合并器件一格，绿色为 Pass，失败按 Hard Bin 上色，叠图时好坏混在一起用琥珀色。旧的 `heatmap_points` 仍在 JSON 里，不再当作主图。 |
| 晶圆筛选 | 从 lot 钻到 wafer | 下拉框过滤 `die_points`。没有坐标的器件不进图，仍留在 KPI 里。 |
| 参数分布与规格尾部 | SiliconDash Distribution Tails | 有上下限时，16 个箱覆盖 `[lo, hi]`，箱外单独计数，并且**每个有限结果都会计入**。没有上下限时最多保留 512 个确定性抽样，并标明 sampled。Pareto 点选测试项时，详情区画出同一张图。 |

人口仍然是 Dashboard 的 coordinate-v1 合并，不是 viewer 的 Latest，也不是 `snapshot analyze` 的 latest-device。显示筛选不改变这套计数。

## 建议但本轮没做

按对日常看报告的价值排序。

1. **多片晶圆廊与叠图。** yieldHUB 可以一次叠几百到上万片，用来看空间图形是否稳定。zstdf 现在是同一 X/Y 叠在一张图上，没有每片缩略图，也没有 reticle / stepper field。适合在 viewer 查询上做，不要把整片原始坐标再塞进一份 HTML。
2. **规格 what-if。** SiliconDash 可以拖动下限看能救回多少良率。这会改变「如果限值不同」的模拟结果，不能改写已经记录的 PRR 结论。应做成单独的模拟层。
3. **跨工序追溯进驾驶舱。** Exensio / yieldHUB 把 WAT、CP、FT 和设备串起来。zstdf 的 traceability 和 `snapshot analyze`（lot / wafer / site / retest）已经分开实现。本轮没有把它们并进 Dashboard，避免混用两套身份规则。
4. **时间趋势和告警。** 商用系统按 lot 时间看良率、Cpk 漂移和 hold。zstdf 有单次运行的 site / wafer 条，没有跨运行的时间轴，也没有告警。需要先有稳定的 snapshot 序列。
5. **湖仓 Gold 层。** Databricks 的做法是原始文件、清洗后的尝试/测试、以及给报表用的聚合各一层。zstdf 已有 Parquet `eav-v2` 和 catalog snapshot，相当于本地 Silver。不建议在这个离线工具里绑定 Delta、Spark 或 Databricks 账号。若以后要对接，只导出已版本化的 Parquet，由外部湖仓建 Gold 表。
6. **CDF、相关性下钻、自动 Insights。** 现有 Correlation 页还在。自动找异常和自然语言解释属于 Exensio Aurora / SiliconDash Insights，依赖上面的稳定聚合，本轮不做。

## 数据侧对照

| 能力 | 参考产品 | zstdf 现状 |
| --- | --- | --- |
| 摄入 | STDF 自动入库、修数、完整性质检 | `convert` / `view` 解码，sanity 另做 CP/FT 字段检查 |
| 存储 | 列存 + 预聚合；Aurora 强调只把图需要的数送到前端 | `eav-v2` Parquet。驾驶舱只下发聚合、直方图和晶粒点 |
| 查询 | 笔记本随意探索 + 固定报表 | 本地 viewer 查询 + 固定 HTML。DuckDB 仍是可选项，不是本轮内容 |
| 身份 | 各家规则不同，但不会把 CP 坐标和 FT 坐标混用 | 保持 Dashboard、viewer、latest-device 三套规则分开 |

## 局限

- 晶圆图不旋转 notch，轴就是源 X/Y。
- 一个合并器件的 Hard Bin 来自该器件第一条记录，与原来的 Bin 汇总一致。
- 没有上下限、且结果超过 512 条的测试，直方图是抽样，图上会写明。
- 公共样例文件不进仓库。复现步骤见 `examples/public-stdf/README.md`。
