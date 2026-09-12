# zstdf

Rust tools for reading semiconductor Standard Test Data Format (STDF) files,
exporting Parquet data, validating records, and generating interactive HTML
dashboards. An optional Python extension exposes the conversion APIs as `_zstdf`.

The independent `traceability` command reads STDF files/directories/gzip directly
and reports device test histories, retest verdict/bin differences, missing steps,
and source evidence in an offline HTML report. See [traceability usage and demo](docs/traceability.md).

The `sanity` command checks CP/FT source records and shows important run fields
plus the first value of the first two records per type in each unit. It exports
field evidence and an offline report. See [CP/FT sanity 安装、运行与示例](docs/sanity.md)
for runnable commands, profile configuration, and the current validation scope.
Quick start: [运行 CP/FT Sanity 报告](#运行-cpft-sanity-报告).

Dashboard part identity now uses **wafer + positive PRR X/Y**, with an all-or-nothing
fallback to **lot + positive integer PTR X/Y**. New conversions produce `eav-v2`;
reconvert older Parquet/catalog datasets from their source STDF before using the
updated dashboard. See [coordinate identity and migration](docs/coordinate_identity.md).

Build instructions: [Windows](#build-on-windows), [RHEL 9](#build-on-red-hat-enterprise-linux-9),
and [macOS](#build-on-macos). The Linux/macOS commands below are setup guidance;
they have not been build-tested on this Windows development machine.

For device histories and retests, follow [安装与运行测试流程追溯报告](#安装与运行测试流程追溯报告)
after building the CLI. This includes a ready-to-run synthetic example and
configuration for your own CP/FT data.

## Build on Windows

These instructions target **64-bit Windows 10/11**. Python is optional for the
command-line tools. Cursor or VS Code can be used as the editor; Microsoft C++
Build Tools are still required for compilation.

### 1. Install Git, Rust, and C++ Build Tools

Run in PowerShell:

```powershell
winget install --id Git.Git -e
winget install --id Rustlang.Rustup -e
```

Install [Microsoft C++ Build Tools](https://learn.microsoft.com/en-us/windows/dev-environment/rust/setup).
In Visual Studio Installer, select **Desktop development with C++** and include:

- MSVC x64/x86 C++ build tools.
- A Windows 10 or Windows 11 SDK.

You do not need to use Visual Studio as your editor. See the
[official Rust installation guide](https://rust-lang.org/tools/install/) for
the Rust installer and Windows prerequisites.

After installation, reopen **Developer PowerShell for Visual Studio** and verify:

```powershell
git --version
rustup default stable-x86_64-pc-windows-msvc
cargo --version
where.exe link
```

The linker path should point to the Microsoft MSVC tools. If no path is returned,
modify the Build Tools installation to add the C++ tools and Windows SDK, then
reopen Developer PowerShell.

### 2. Clone the Repository

If moving from another computer, ensure the changes you need have been committed
and pushed there first. Cloning only retrieves changes available on GitHub.

```powershell
git clone https://github.com/zefangzh/zstdf.git
cd zstdf
```

Run all following build commands from this folder, which contains the top-level
`Cargo.toml`. The first build needs internet access to download dependencies.

### 3. Test and Build the CLI

These commands exclude the optional Python binding from testing:

```powershell
cargo test --workspace --exclude stdf-py --locked
cargo build --release -p stdf-cli --locked
```

The executable is `target\release\zstdf-cli.exe`.

Check that the new command is available:

```powershell
.\target\release\zstdf-cli.exe traceability --help
```

To optionally install the executable into Cargo's bin directory, run
`cargo install --path .\stdf-cli --locked` from the repository root. With
`$HOME\.cargo\bin` on PATH, you can then use `zstdf-cli` from other folders.
Installing this way is optional; the examples below use the release executable
inside the repository.

## Build on Red Hat Enterprise Linux 9

Use Bash on an x86_64 or aarch64 RHEL 9 machine with access to its BaseOS and
AppStream repositories. Install the native compiler and build utilities:

```bash
sudo dnf install -y git gcc gcc-c++ make pkgconf-pkg-config ca-certificates
```

Check that `curl` is available (`curl --version`). If it is missing, install
`curl-minimal` with `sudo dnf install -y curl-minimal`. See Red Hat's
[C/C++ development guide](https://docs.redhat.com/en/documentation/red_hat_enterprise_linux/9/html-single/developing_c_and_cpp_applications_in_rhel_9/developing_c_and_cpp_applications_in_rhel_9)
for compiler setup.

Install current stable Rust for your native architecture using
[rustup](https://rust-lang.org/tools/install/), as your regular user:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/zstdf-rustup-init.sh
sh /tmp/zstdf-rustup-init.sh --default-toolchain stable
. "$HOME/.cargo/env"
rustup default stable
cargo --version
gcc --version
```

Clone, test, and build from the repository root:

```bash
git clone https://github.com/zefangzh/zstdf.git
cd zstdf
cargo test --workspace --exclude stdf-py --locked
cargo build --release -p stdf-cli --locked
./target/release/zstdf-cli --help
```

Python is not needed for these CLI build/test commands. The executable is
`target/release/zstdf-cli`, without an `.exe` suffix. Do not use the Windows
`stable-x86_64-pc-windows-msvc` toolchain on Linux.

## Build on macOS

Use Terminal's native Zsh or Bash on your current macOS release. On Apple
silicon, use an arm64 terminal and matching Python installation; avoid mixing
Rosetta/x86_64 tools with native arm64 tools. On an Intel Mac, use x86_64 tools.

Install [Apple's Command Line Tools](https://developer.apple.com/documentation/xcode/installing-the-command-line-tools/):

```bash
xcode-select --install
```

Finish the installer before continuing. If the tools are already installed,
verify them and update them through Software Update when needed:

```bash
xcode-select -p
clang --version
git --version
uname -m
```

Install Rust with [rustup](https://rust-lang.org/tools/install/). It selects the
native macOS toolchain; the Windows MSVC toolchain is not needed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/zstdf-rustup-init.sh
sh /tmp/zstdf-rustup-init.sh --default-toolchain stable
. "$HOME/.cargo/env"
rustup default stable
cargo --version
```

Clone, test, and build:

```bash
git clone https://github.com/zefangzh/zstdf.git
cd zstdf
cargo test --workspace --exclude stdf-py --locked
cargo build --release -p stdf-cli --locked
./target/release/zstdf-cli --help
```

The executable is `target/release/zstdf-cli`. Homebrew and Python are optional
for the CLI; only the Python setup below uses Homebrew.

## Run the CLI

### Phase 10D 以前的功能是否仍然保留？

**保留。** `sanity` 和 `traceability` 是新增的独立入口，原有 CLI 命令与 Python
转换接口仍然存在。当前可用入口如下；这里表示已实现的功能，不代表整个 Phase 10D 已完成。

| 功能 | 当前入口 |
| --- | --- |
| STDF 解码摘要、记录查看、ASCII 导出 | `info`、`dump`、`to-ascii` |
| 原有单文件和批量校验 | `check`、`batch-check` |
| 单文件 Parquet 转换、manifest 和重试复用 | `convert`、`--no-overwrite` |
| Phase 10A 单文件可视化 | `dashboard` |
| Phase 10B 多文件转换和文件粒度分区 | `convert-many` |
| Phase 10C.1 按行分区和受预算约束的转换 | `convert-partitioned`（当前支持 PTR） |
| Phase 10C.2 数据集完整性、恢复与多 lot 可视化 | `verify-dataset`、`recover-dataset`、`dashboard-dir` |
| 可选 Python 转换接口 | `_zstdf`，见 [Optional Python Binding](#optional-python-binding) |

原来的 `STDF → convert → dashboard` 和
`STDF → convert-partitioned → verify-dataset → dashboard-dir` 流程仍然可用。
`sanity` 直接读取 STDF，输出的 `record_fields.parquet` 是字段检查证据，不能作为
`dashboard` 所需的测量 EAV Parquet。它也不会自动调用转换或修改 Dashboard 的良率口径。

旧版 Parquet/catalog 必须从 STDF 重新转换到新的输出目录，得到 `eav-v2` 后再使用
当前 Dashboard；这是坐标身份规则升级的兼容要求。Phase 10D.2/10D.3 和 10D.5
仍未完成，10D.4 sanity 已有可运行实现但尚未完成全部标准规则验收。
详细状态见 [阶段规格](PHASED_EXECUTION_SPEC.md)。

### Windows PowerShell

Replace the example paths below with files or directories that exist on your
computer. STDF input supports plain files and gzip compression.

Show available commands and inspect a file:

```powershell
.\target\release\zstdf-cli.exe --help
.\target\release\zstdf-cli.exe info C:\data\input.stdf
```

Convert to Parquet, then generate and open an interactive dashboard:

```powershell
.\target\release\zstdf-cli.exe convert C:\data\input.stdf output.parquet
.\target\release\zstdf-cli.exe dashboard output.parquet dashboard.html
Start-Process .\dashboard.html
```

The dashboard command requires an existing Parquet file. Run conversion
successfully before generating the dashboard.

For bounded, mixed-lot/wafer PTR conversion from a directory:

```powershell
.\target\release\zstdf-cli.exe convert-partitioned --output-dir dataset C:\data\inputs
```

This workflow writes multiple Parquet fragments and supports resource limits.
It currently supports PTR results; MPR/FTR expansion returns an explicit error.
Its memory budget applies to accounted conversion state, not an OS-enforced
resident-memory ceiling. Use `dashboard` for one Parquet file or `dashboard-dir`
for a catalog-backed dataset. See [CLI documentation](docs/cli.md) for resource options, retries, and
dataset-generation handling.

Quote paths containing spaces, for example `"C:\test data\input.stdf"`.

### RHEL 9 and macOS

Use forward-slash paths and the native executable without `.exe`. Replace
`/path/to/input.stdf` and `/path/to/inputs` with actual paths:

```bash
./target/release/zstdf-cli info /path/to/input.stdf
./target/release/zstdf-cli convert /path/to/input.stdf output.parquet
./target/release/zstdf-cli dashboard output.parquet dashboard.html
./target/release/zstdf-cli convert-partitioned --output-dir dataset /path/to/inputs
```

Open the generated dashboard on macOS:

```bash
open dashboard.html
```

On a RHEL graphical desktop with `xdg-open` installed:

```bash
xdg-open dashboard.html
```

On a headless RHEL server, transfer `dashboard.html` to your desktop computer
and open it in a browser. The generated HTML is self-contained; no web server
is required. The same PTR-only and memory-accounting limits described above apply.

## 运行 CP/FT Sanity 报告

在仓库根目录运行。先按上方对应平台的安装说明准备 Rust 和编译工具；读取实际 STDF
只需要 CLI，Python 仅用于生成下面的合成示例。

### 1. 构建并确认入口

```powershell
cargo build --release -p stdf-cli --locked
.\target\release\zstdf-cli.exe sanity --help
```

### 2. 检查实际 CP 或 FT 数据

把输入路径换成实际存在的文件或目录。支持多个输入、目录递归和 gzip。
未指定 `--profile` 时，使用对应 CP/FT 的内置基础规则。

```powershell
.\target\release\zstdf-cli.exe sanity "D:\STDF\CP" `
  --test-domain cp --output-dir .\reports\cp-sanity
Invoke-Item .\reports\cp-sanity\report.html

.\target\release\zstdf-cli.exe sanity "D:\STDF\FT\sample.stdf.gz" `
  --test-domain ft --output-dir .\reports\ft-sanity
Invoke-Item .\reports\ft-sanity\report.html
```

CP 基础规则要求明确的 wafer 上下文；FT 不因缺少 wafer 记录自动报错。
实际产品格式可通过 `--profile .\my-cp-profile.json` 配置。
[CP 示例](examples/sanity/cp-profile.json) 和 [FT 示例](examples/sanity/ft-profile.json)
中的 lot/程序命名用于合成数据，请修改后再用于产品数据。

### 3. 运行合成示例及混合 CP/FT 报告

以下使用 Python 3 标准库生成 STDF，不需要安装 Python binding：

```powershell
python .\examples\sanity\generate_demo.py

.\target\release\zstdf-cli.exe sanity .\examples\sanity\generated\cp.stdf `
  --test-domain cp --profile .\examples\sanity\cp-profile.json `
  --output-dir .\examples\sanity\generated\cp-report

.\target\release\zstdf-cli.exe sanity `
  .\examples\sanity\generated\cp.stdf .\examples\sanity\generated\ft.stdf `
  --run-profiles .\examples\sanity\run-profiles.json `
  --output-dir .\examples\sanity\generated\mixed-report

Invoke-Item .\examples\sanity\generated\mixed-report\report.html
```

混合示例包含 **2 个 run、6 个 unit**。FT 的 3 个身份未解析 warning 是演示数据的预期结果，
不会被报告为格式验证错误。`--run-profiles` 与 `--test-domain`、`--profile` 互斥。
每个 MIR 必须唯一匹配一条路由；示例按 `JOB_NAM` 和 `JOB_REV` 精确匹配，
使用真实数据时要修改 [run-profiles.json](examples/sanity/run-profiles.json)。

报告上半部展示每个 run 的 MIR/SDR 等重要字段；选择 unit 后，按 DTR/PTR/MPR/FTR/GDR
类型各展示前两条记录的首值。所有记录仍参与检查，原始值、默认/继承来源和诊断保留在证据中。
浏览器支持 run 切换、unit 搜索、诊断筛选和完整 JSON 导出，不需要 Web 服务。
首值以数值、单位或 PASS/FAIL 显示；点击 **Raw evidence** 查看原始 bits、字段状态及来源。
显示格式不会改变原始证据或 JSON 导出，run 下拉框同时标明 CP/FT 与 profile ID。

`--output-dir` 自动创建。根目录的 `report.html` 引用同目录下不可变的 `sanity-*` 子目录；
分享全部证据时应复制整个输出目录。检查发现错误时会发布明确失败的诊断报告并返回退出码 1；
I/O、配置错误、超限或取消则保留原报告。生成的 `ft-invalid.stdf` 含一条位于预览范围之外的
NaN，可用于验证全文件检查。

### 4. Linux/macOS

构建方式见上方平台说明，参数相同，使用原生可执行文件路径：

```bash
./target/release/zstdf-cli sanity /path/to/cp \
  --test-domain cp --output-dir reports/cp-sanity
```

macOS 用 `open reports/cp-sanity/report.html`；Linux 桌面用
`xdg-open reports/cp-sanity/report.html`。本地实际验证环境为 Windows，
Linux/macOS 命令是对应平台的运行说明。资源限制和完整字段合同见 [Sanity 说明](docs/sanity.md)。

## 安装与运行测试流程追溯报告

`traceability` 直接读取多份 STDF，按 wafer/坐标关联芯片的各步骤与重测，生成可离线
打开的交互式 HTML。支持文件、目录和 gzip；不需要先转换 Parquet，也不需要 Python
或启动 Web 服务。每次 PRR 都作为独立测试保留，内容相同的文件副本自动去重。

### 1. 安装和构建（Windows）

先按上面的 [Windows 安装说明](#build-on-windows) 安装 Git、Rust、MSVC C++
Build Tools 和 Windows SDK，然后在 Developer PowerShell 中运行：

```powershell
git clone https://github.com/zefangzh/zstdf.git
cd zstdf
cargo test --workspace --exclude stdf-py --locked
cargo build --release -p stdf-cli --locked
.\target\release\zstdf-cli.exe traceability --help
```

如果已经克隆过仓库，在现有仓库根目录执行构建命令即可，不必再次克隆。
首次构建需要联网下载依赖；依赖已经缓存后，可以为 Cargo 命令添加 `--offline`。
CLI 源码构建不需要安装 Python binding。

### 2. 运行仓库自带示例

在仓库根目录运行以下 PowerShell 命令。示例 STDF 已随仓库提供，无需安装 Python：

```powershell
.\target\release\zstdf-cli.exe traceability `
  .\examples\traceability\generated\inputs `
  --flow-config .\examples\traceability\flow.json `
  --flow-closures .\examples\traceability\closures.json `
  --identity-map .\examples\traceability\identities.json `
  --output .\trace.html

Invoke-Item .\trace.html
```

PowerShell 的反引号必须是每行最后一个字符，后面不要加空格。也可以把命令合并成
一行。成功时输出 **10 个器件、9 份独立 STDF**；其中一个 gzip 副本不会增加重测次数。
示例步骤 `stage-a`、`stage-b`、`stage-c` 是演示配置，不代表实际产品流程。

浏览器中可按 wafer/lot、X/Y 坐标、步骤和异常类型筛选；点击矩阵单元格下钻，查看
各步骤全部测试记录、程序/设备、源文件、SHA-256 和记录位置。重测判定/bin 变化与
缺失步骤会用文字、图标和颜色高亮。“导出全部 JSON 证据”导出完整数据，不受当前筛选影响。

### 3. 为实际 CP/FT 数据配置流程

先复制示例配置并编辑，目标文件名可自行调整：

```powershell
Copy-Item .\examples\traceability\flow.json .\flow.json
notepad .\flow.json
```

修改流程 `flow_id`、`version`、有序 `steps`，以及各步骤的 `required`、`stop_on_fail`
和 `matches`。匹配字段取自 STDF 的 MIR：`job_nam`、`job_rev`、`test_cod`、`flow_id`、
`tst_temp`。字段采用**区分大小写的精确匹配**；同一条件内各字段为 AND，多组条件为 OR。
不同程序版本可以配置为同一步骤，未匹配或匹配多个步骤时会显示“步骤未识别”。

需要核对实际 MIR 字段时，可以先导出记录文本：

```powershell
.\target\release\zstdf-cli.exe to-ascii "D:\STDF\CP\sample.stdf" .\sample-records.txt
notepad .\sample-records.txt
```

将以下路径换成真实存在的数据目录或文件，再生成报告：

```powershell
.\target\release\zstdf-cli.exe traceability `
  "D:\STDF\CP" "D:\STDF\FT" `
  --flow-config .\flow.json `
  --output .\trace.html

Invoke-Item .\trace.html
```

输入可以混合多个文件与目录，含空格的路径必须加引号。输出目录需要提前存在。
再次成功运行会原子替换同名报告；输入错误、超限或协作取消会保留已有报告。

### 4. 可选：显式关闭流程与映射器件身份

- `--flow-closures closures.json`：指定流程 ID/版本以及 lot 或完整器件身份，明确流程
  已关闭。未关闭时，尚未进入的末尾必测步骤显示“待测”；已有后续步骤的前置必测缺口
  可以判为“缺失”。单个 STDF 的 MRR 不等于制造流程关闭。
- `--identity-map identities.json`：显式将完整的 lot/PTR 键映射到 wafer/PRR 键。
  不会只因坐标相同就自动跨 wafer/lot 关联。无映射的回退身份显示关联限制。

编辑好这两个可选配置文件后运行：

```powershell
.\target\release\zstdf-cli.exe traceability `
  "D:\STDF\CP" "D:\STDF\FT" `
  --flow-config .\flow.json `
  --flow-closures .\closures.json `
  --identity-map .\identities.json `
  --output .\trace.html
```

配置格式参见 [流程](examples/traceability/flow.json)、
[关闭清单](examples/traceability/closures.json)、[身份映射](examples/traceability/identities.json)。
请按实际产品填写，不要将演示关闭清单或映射直接用于真实数据。

### 5. Linux/macOS 与重新生成合成数据

完成上面的对应平台构建后，可在仓库根目录运行示例：

```bash
./target/release/zstdf-cli traceability examples/traceability/generated/inputs \
  --flow-config examples/traceability/flow.json \
  --flow-closures examples/traceability/closures.json \
  --identity-map examples/traceability/identities.json \
  --output trace.html
```

macOS 用 `open trace.html` 打开；Linux 图形桌面可用 `xdg-open trace.html`。
无桌面的服务器可以将 HTML 复制到本机浏览器打开。这些原生命令示例尚未在 Linux/macOS
验证；本次实际构建与浏览器验证在 Windows 完成。

如果需要重新生成合成 STDF 和演示报告，可使用可选的 Python 3 脚本：

```powershell
python .\examples\traceability\generate_demo.py --cli .\target\release\zstdf-cli.exe
Invoke-Item .\examples\traceability\generated\demo.html
```

完整判定规则、资源预算和证据字段见 [追溯报告说明](docs/traceability.md)。本地验证包括
291 项 Rust 测试、Release 构建、Python binding 编译，以及桌面/手机尺寸的浏览器交互验证；
测试使用合成 STDF，实际产品流程及 tester 数据需单独验证。

## Command and Parameter Reference

Below, `zstdf-cli` means `.\target\release\zstdf-cli.exe` on Windows or
`./target/release/zstdf-cli` on Linux/macOS. Alternatively, run commands from
the repository root with `cargo run -p stdf-cli -- <command> <arguments>`.
Angle brackets mark required values; square brackets mark optional arguments.
Do not type the brackets. Quote paths and titles containing spaces.

Use `zstdf-cli --help` to list commands, `zstdf-cli --version` for the version,
and `zstdf-cli <command> --help` for that command's options. Boolean flags are
off unless supplied; use `--no-overwrite`, not `--no-overwrite true`.

### `sanity`: Inspect CP/FT Fields and Compact Unit Previews

```text
zstdf-cli sanity <inputs>... --test-domain cp|ft --output-dir DIR [--profile FILE]
zstdf-cli sanity <inputs>... --run-profiles FILE --output-dir DIR
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `inputs` | Required, one or more | STDF files/directories; gzip supported. |
| `--test-domain cp\|ft` | Required unless `--run-profiles` is supplied | One domain for all input runs. |
| `--profile FILE` | Built-in domain profile | Product rules for the selected domain. |
| `--run-profiles FILE` | None | Unique per-MIR route selection for mixed CP/FT; conflicts with the two options above. |
| `--output-dir DIR` | Required | Bundle directory; open its root `report.html`. |
| `--preview-records-per-type N` | `2` | First N records per type in each unit, range 1–100; does not limit full-file checking. |
| `--max-report-mib N` | `32` | Retained-summary budget and serialized report limit, not a hard RSS cap. |
| `--disk-limit-mib N` | `1024` | Write budget for this generation, including evidence and atomic report copy. |
| `--max-units N` | `100000` | Maximum independent unit attempts; exceeding it fails without truncation. |
| `--max-sources N` | `10000` | Input-path limit before content deduplication. |
| `--cancel-file FILE` | None | Creating this file requests cooperative cancellation before publication. |

### `traceability`: Trace Steps and Retest Differences

```text
zstdf-cli traceability <inputs>... --flow-config flow.json --output trace.html [options]
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `inputs` | Required, one or more | STDF files/directories; gzip supported. |
| `--flow-config FILE` | Required | Flow ID/version, ordered steps and exact MIR selectors. |
| `--output FILE` | Required | Self-contained HTML report; parent directory must exist. |
| `--flow-closures FILE` | None | Explicit lot/device closures for the configured flow ID/version. |
| `--identity-map FILE` | None | Full lot/PTR-to-wafer/PRR identity mappings; conflicts fail. |
| `--memory-limit-mib N` | `256` | Accounted working-data budget, minimum 16 MiB; not a hard RSS cap. |
| `--disk-limit-mib N` | `1024` | Live sort scratch plus reserved report staging; must exceed report limit. |
| `--max-report-mib N` | `32` | Maximum complete HTML size; positive and at most memory limit / 4. |
| `--max-devices N` | `100000` | Maximum reported devices; no silent truncation. |
| `--max-sources N` | `10000` | Maximum source paths, including duplicate-content aliases. |
| `--temp-dir DIR` | OS temporary directory | Existing directory for bounded sort scratch. |
| `--cancel-file FILE` | None | Creating this file requests cooperative cancellation; checked before commit. |

Unknown ordering does not imply a final result. The report retains every PRR
attempt and highlights verdict/bin differences across all attempts. See
[traceability rules and limitations](docs/traceability.md) for missing/pending,
failure stops, identity fallback and cancellation behavior.

### `info`: Inspect a File

```text
zstdf-cli info <input>
```

| Parameter | Meaning |
| --- | --- |
| `input` | Required STDF file path; plain or gzip-compressed. |

Prints byte order, record counts, bytes consumed, truncation status, and decode
errors. This is an inspection command: reported decode errors do not themselves
make its exit status fail. Use `check` for validation in automation.

### `dump`: Print Decoded Records

```text
zstdf-cli dump <input> [--limit N]
zstdf-cli dump input.stdf --limit 20
zstdf-cli dump input.stdf > records.txt
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `input` | Required | STDF input file. |
| `--limit N` | No limit | Maximum decoded records to print, not bytes or test results. `0` prints none. |

Writes compact record text to standard output. There is no output-path argument;
use shell redirection as shown, or `to-ascii` for reference-style formatting.

### `check`: Validate STDF Structure

```text
zstdf-cli check <input>
```

| Parameter | Meaning |
| --- | --- |
| `input` | Required STDF file to decode and validate. |

Prints a summary and findings, including severity, rule, record, and offset.
Checks include record pairing, lot metadata, abort detection, PTR limits/results,
test-name consistency, and hardware-bin counts. Validation errors produce a
nonzero exit status.

### `to-ascii`: Export Reference-Style Text

```text
zstdf-cli to-ascii <input> [output] [--debug]
zstdf-cli to-ascii input.stdf decoded.txt
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `input` | Required | STDF file to export. |
| `output` | Input path with its last extension replaced by `.txt` | Destination text file. `input.stdf` becomes `input.txt`; `input.stdf.gz` becomes `input.stdf.txt`. |
| `--debug` | Off | Accepted for legacy compatibility; currently does not change output or enable extra logging. |

Includes reference-style record formatting, header/footer, scales, timestamps,
and generic data formatting. Existing output is overwritten; its parent directory
must already exist. The complete text is assembled in memory before writing,
so this command is not a bounded-memory export.

### `batch-check`: Validate a Directory

```text
zstdf-cli batch-check <root-dir> [report-path] [--threads N]
zstdf-cli batch-check inputs reports.txt --threads 2
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `root-dir` | Required | Directory to scan recursively for `.std`, `.stdf`, `.std.gz`, and `.stdf.gz` files, case-insensitively. Symbolic-link entries are skipped. |
| `report-path` | `<root-dir>/sanity_report.txt` | Consolidated text report; an existing report is overwritten. |
| `--threads N` | `1` | Parallel validation workers. `0` is treated as `1`, not automatic CPU detection. |

Prints file/failure counts and the report path. Any failed input produces a
nonzero exit status. More workers can increase memory usage, especially for gzip
inputs; use one worker when diagnosing failures or working with limited RAM.

### `convert`: Produce One Parquet File

```text
zstdf-cli convert <input> <output> [--batch-size N] [--no-overwrite]
zstdf-cli convert input.stdf output.parquet --batch-size 4096 --no-overwrite
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `input` | Required | Source STDF file. |
| `output` | Required | Destination long-format EAV Parquet file. |
| `--batch-size N` | `65536` | Target EAV rows per Arrow batch; `0` becomes `1`. Part completion can exceed this target, so it is not a hard memory limit. |
| `--no-overwrite` | Off | If output exists, return its existing manifest summary instead of replacing it. |

Writes a companion `<output>.json` manifest and prints batch/row counts. By
default an existing output may be replaced. The no-overwrite fast path requires
a readable manifest but does not reopen the input or perform full integrity
verification. It is not proof that an existing output matches a changed source.

### `convert-many`: File-Level Partitioning

```text
zstdf-cli convert-many <inputs>... --output-dir <dir> [--partition-by KEYS] [--batch-size N] [--no-overwrite]
zstdf-cli convert-many inputs --output-dir dataset --partition-by lot-id,wafer-id
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `inputs` | Required, one or more | Space-separated files or recursively scanned directories. Directory scanning uses the STDF extensions listed under `batch-check`. |
| `--output-dir DIR` | Required | Root directory for generated Parquet files and manifests. |
| `--partition-by KEYS` | `input-file` | Comma-separated `input-file`, `lot-id`, or `wafer-id` keys, in directory nesting order. |
| `--batch-size N` | `65536` | Target EAV rows per batch, not a RAM cap. |
| `--no-overwrite` | Off | Reuse matching existing output only after source/manifest and Parquet metadata checks. The source must remain available. |

Canonical input paths are deduplicated. This produces one Parquet file per source;
selected lot/wafer keys must be consistent within each source. Use
`convert-partitioned` for a source containing multiple lots or wafers. Output names
include source identity information; changed source content can create new files.

### `convert-partitioned`: Bounded Dataset Conversion

```text
zstdf-cli convert-partitioned <inputs>... --output-dir <dir> [options]
zstdf-cli convert-partitioned inputs --output-dir dataset --memory-limit-mib 256 --row-group-rows 4096
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `inputs` | Required, one or more | STDF files or directories to scan recursively. |
| `--output-dir DIR` | Required | Catalog-backed dataset root containing Parquet fragments and `_catalog.json`. |
| `--partition-by KEYS` | `lot-id,wafer-id` | Comma-separated partition keys: `input-file`, `lot-id`, `wafer-id`. Routes rows rather than entire input files. |
| `--memory-limit-mib N` | `256` | Budget for accounted conversion state, in MiB (1,048,576 bytes). Not a hard process RSS limit. |
| `--max-pending-tests N` | `100000` | Maximum pending test results across unfinished parts. |
| `--max-open-writers N` | `4` | Maximum simultaneously open fragment writers. |
| `--row-group-rows N` | `65536` | Maximum rows per row group and fragment. Smaller values can create more output files. |
| `--max-output-files N` | `10000` | Maximum generated fragments for an invocation; also affects reserved metadata memory. |
| `--continue-on-error` | Off | Continue processing other inputs after an input fails, rather than stopping at the first failure. Any failures still result in a nonzero CLI exit status. |

Resource limits must be positive. Currently supports PTR results; unsupported
MPR/FTR expansion is an explicit error. Prints rows, fragments, writer/memory
peaks, and failures. Inspect `_catalog.json` for run status and failed inputs.
This command has no `--no-overwrite` option; dataset identity and current versions
are managed through its catalog.

### `verify-dataset`: Check Dataset Integrity

```text
zstdf-cli verify-dataset <input>
zstdf-cli verify-dataset dataset
```

| Parameter | Meaning |
| --- | --- |
| `input` | Required catalog-backed dataset root, not an individual Parquet file. |

Verifies current snapshot paths, hashes, schemas, and row counts. Prints source
count, catalog revision, and run status; verification failures return nonzero.

### `recover-dataset`: Clean Interrupted Work

```text
zstdf-cli recover-dataset <input>
zstdf-cli recover-dataset dataset
```

| Parameter | Meaning |
| --- | --- |
| `input` | Required dataset root to recover after an interrupted conversion. |

This command **modifies the dataset**: it removes abandoned `.staging-*`
directories, recoverable stale writer markers, and temporary catalog files under
the managed paths. It checks writer ownership/locks and refuses active writers.
A catalog run left as `running` is marked `interrupted`. It does not delete
completed generations or reconstruct missing/corrupt committed Parquet files.
Run `verify-dataset` afterward before consuming the data.

### `dashboard`: Visualize One Parquet File

```text
zstdf-cli dashboard <input> <output> [--title TEXT] [--max-correlation-tests N]
zstdf-cli dashboard output.parquet dashboard.html --title "Lot DataView" --max-correlation-tests 24
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `input` | Required | Existing EAV Parquet file, such as output from `convert`; not an STDF file or dataset directory. |
| `output` | Required | Destination self-contained interactive HTML file. |
| `--title TEXT` | `zstdf DataView` | Dashboard title; quote text containing spaces. |
| `--max-correlation-tests N` | `16` | Maximum numeric tests considered for correlation analysis. Values below `2` become `2`. Does not limit all input rows/tests loaded. |

Prints row/part counts and yield percentage. This single-file command does not
provide a memory-limit flag. Open the generated HTML in a browser as described
in the platform examples above.

### `dashboard-dir`: Visualize a Catalog Dataset

```text
zstdf-cli dashboard-dir <input> <output> [--title TEXT] [--memory-limit-mib N] [--max-lots N] [--max-parts N]
zstdf-cli dashboard-dir dataset dashboard.html --title "Dataset DataView" --max-lots 64
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `input` | Required | Catalog-backed dataset root from `convert-partitioned`, not an arbitrary folder of Parquet files. |
| `output` | Required | Destination self-contained HTML dashboard with lot selection. |
| `--title TEXT` | `zstdf Dataset DataView` | Dashboard title. |
| `--memory-limit-mib N` | `256` | Accounted analysis-memory budget in MiB; minimum `1`. Not a hard RSS cap. |
| `--max-lots N` | `32` | Maximum distinct lots allowed in the analysis; must be positive. |
| `--max-parts N` | `100000` | Maximum distinct parts across the analysis; must be positive. |

Reads current catalog versions rather than mixing historical generations.
Resource limits cause errors instead of silently truncating the analysis.
Unlike `dashboard`, this command has no `--max-correlation-tests` CLI option.

## Optional Python Binding

### Windows

Install **64-bit Python 3.12** with the Windows Python launcher (`py`). The Rust
and C++ build tools above are also required to build the extension from source.

From the repository root, create a virtual environment and install the package:

```powershell
py -3.12 -m venv .venv
$env:PYO3_PYTHON = (Resolve-Path .\.venv\Scripts\python.exe).Path
.\.venv\Scripts\python.exe -m pip install --upgrade pip
.\.venv\Scripts\python.exe -m pip install pyarrow .
.\.venv\Scripts\python.exe -c "import _zstdf; print(_zstdf.__version__)"
```

Using the virtual environment's Python directly avoids needing to activate it.
The installed distribution is named `stdf-rs`; its import name is `_zstdf`.

Example Python usage:

```python
import _zstdf

rows = _zstdf.write_parquet(r"C:\data\input.stdf", "output.parquet")
print(f"Converted {rows} rows")
```

To run the complete Rust workspace tests, including the Python binding, make
the base Python DLL directory available in the current shell:

```powershell
$env:PYO3_PYTHON = (Resolve-Path .\.venv\Scripts\python.exe).Path
$pythonBase = py -3.12 -c "import sys; print(sys.base_prefix)"
$env:Path = "$pythonBase;$env:Path"
cargo test --workspace --locked
```

### RHEL 9

Python 3.12 packages are available starting with **RHEL 9.4**. These optional
commands assume access to a RHEL 9 repository version providing those packages;
they are not required for the CLI on earlier RHEL 9 releases. See Red Hat's
[Python installation guide](https://developers.redhat.com/blog/install-python3-rhel).

```bash
sudo dnf install -y python3.12 python3.12-pip python3.12-devel
python3.12 -m venv .venv
export PYO3_PYTHON="$PWD/.venv/bin/python"
.venv/bin/python -m pip install --upgrade pip
.venv/bin/python -m pip install pyarrow .
.venv/bin/python -c "import _zstdf; print(_zstdf.__version__)"
```

Run these commands from the repository root. If the Python development package
is unavailable, ask your RHEL administrator to enable the appropriate approved
repositories; do not replace the OS-managed Python installation.

### macOS

If you do not already have Python 3.12, install [Homebrew](https://brew.sh/) and
follow its shell/PATH setup instructions. Then install its versioned
[Python 3.12 formula](https://formulae.brew.sh/formula/python%403.12):

```bash
brew install python@3.12
"$(brew --prefix python@3.12)/bin/python3.12" -m venv .venv
export PYO3_PYTHON="$PWD/.venv/bin/python"
.venv/bin/python -m pip install --upgrade pip
.venv/bin/python -m pip install pyarrow .
.venv/bin/python -c "import _zstdf; print(_zstdf.__version__)"
```

Run from the repository root. `brew --prefix` avoids hard-coding different
Homebrew locations on Apple silicon and Intel Macs. The virtual environment
keeps project dependencies separate from Homebrew's managed Python packages.

For Linux/macOS, verify the installed extension through the import smoke test
above and run the CLI-only Rust test command shown in the platform setup.
The binding currently enables PyO3's `extension-module` feature; full Rust
workspace test linking on Unix is not validated by these instructions.

Example conversion on either platform:

```bash
.venv/bin/python -c "import _zstdf; print(_zstdf.write_parquet('/path/to/input.stdf', 'output.parquet'))"
```

### Python Function Parameters

The Python module uses underscores in parameter names. File paths are strings;
`input_paths` and `partition_by` are lists of strings, not comma-separated text.
The multi-file Python functions take explicit file paths, unlike the CLI's
recursive directory expansion.

| Function | Parameters and defaults | Return value |
| --- | --- | --- |
| `read_batches(path, batch_size)` | `path`: STDF source. Pass `None` for the default batch size of `65536`; `0` becomes `1`. | List of PyArrow record batches. All batches are collected in memory; this is not a streaming iterator. |
| `write_parquet(input_path, output_path, batch_size=None, overwrite=None)` | Required source/destination paths. `batch_size=None` means `65536`. `overwrite=None` means `True`; `False` uses the same existing-manifest fast path as CLI `convert --no-overwrite`. | Number of rows. |
| `write_parquet_many(input_paths, output_dir, batch_size=None, overwrite=None, partition_by=None)` | Required source list/destination directory. Batch and overwrite defaults as above. `partition_by=None` means `["input-file"]`; `[]` disables partition directories. | `(files, rows)`. |
| `write_parquet_partitioned(input_paths, output_dir, partition_by=None, memory_limit_mib=256, max_pending_tests=100000, max_open_writers=4, row_group_rows=65536, max_output_files=10000)` | Required source list/dataset root. `partition_by=None` means `["lot-id", "wafer-id"]`; `[]` disables partition directories. Resource parameters have the same meanings as their hyphenated CLI counterparts above. No `overwrite` or `continue_on_error` parameter. | `(files, rows, fragments)`. |

Example with explicit parameters:

```python
import _zstdf

batches = _zstdf.read_batches("input.stdf", 4096)
files, rows, fragments = _zstdf.write_parquet_partitioned(
    ["input.stdf"],
    "dataset",
    partition_by=["lot-id", "wafer-id"],
    memory_limit_mib=256,
    max_open_writers=2,
    row_group_rows=4096,
)
```

## Troubleshooting

| Error | What to check |
| --- | --- |
| `cargo` is not recognized | Reopen the shell after Rust installation. Verify `$HOME\.cargo\bin\cargo.exe` exists and `$HOME\.cargo\bin` is on PATH. |
| `link.exe` not found | Install the MSVC C++ tools and Windows SDK, then use Developer PowerShell. Cursor/VS Code alone does not provide the linker. |
| Could not find `Cargo.toml` | Change directory to the cloned repository root before running Cargo. |
| Python/PyO3 build errors | For CLI-only use, exclude `stdf-py` from workspace tests. For Python use, select the 64-bit Python 3.12 environment with `PYO3_PYTHON` as shown above. |
| File not found when generating a dashboard | Confirm conversion succeeded and supply the actual generated Parquet path. |
| Unknown `convert-partitioned` command | Ensure your checkout includes Phase 10C.1 and rebuild the release CLI. |
| Unknown `traceability` command | Update the checkout to a revision containing traceability, then run `cargo build --release -p stdf-cli --locked`. Use the executable from that checkout. |
| 追溯报告显示“步骤未识别” | 检查实际 MIR 字段与 `matches` 的大小写、程序版本和字段值；避免同一记录匹配多个步骤。 |
| 末尾步骤显示“待测” | 这是未关闭流程的正常状态；只有确认流程结束后才提供对应关闭清单。 |
| 回退身份关联受限 / 身份未解析 | 检查 wafer/PRR 坐标与 PTR 坐标候选；需要跨命名空间关联时提供完整身份映射。 |
| Traceability resource limit exceeded | Adjust the reported limit together with related memory/disk/report constraints, or split inputs by the intended flow population. No partial report is published. |
| `cargo: command not found` on Linux/macOS | Run `. "$HOME/.cargo/env"` or reopen your terminal after rustup installation. |
| `cc` or `clang` not found | On RHEL install GCC/build utilities; on macOS install the Command Line Tools and verify `xcode-select -p`. |
| RHEL cannot find Python 3.12 packages | Check that the enabled repositories provide RHEL 9.4 or later packages. Python is optional for the CLI. |
| Wrong architecture on macOS | Use matching native architectures for the terminal, Rust, Homebrew, and Python. Recreate the virtual environment if its Python architecture is wrong. |

## Project Documentation

### Phase 10D Development Tools

The `stdf-analytics` crate currently provides a bounded disk-backed sorting
foundation, not a replacement dashboard engine. Existing dashboard resource
limits are unchanged. Run its regression suite and scratch-storage stress tool:

```text
cargo test -p stdf-analytics --offline
cargo run -p stdf-analytics --bin spill_stress -- 100000
```

`spill_stress [rows]` takes an optional nonnegative record count (default
`100000`). It uses the OS temporary directory, a 1 MiB accounted memory budget,
a 256 MiB scratch-file budget, and merge fan-in 8. It checks stable ordering,
row counts, and cleanup, then prints peak scratch bytes and elapsed milliseconds.
It does not read STDF inputs or modify datasets. Forced termination may leave an
owned `.zstdf-analytics-*` scratch directory; automated recovery is not yet provided.
On Windows, run `powershell -File scripts/measure_analytics_memory.ps1` for the
100,000/1,000,000-record RSS regression check. It builds the runner, preserves logs
under `target`, and checks a 64 MiB peak/16 MiB growth allowance by default.
See the phased spec for dashboard integration and operational validation gates.

- [CLI and Python usage](docs/cli.md)
- [Phased execution spec and implementation status](PHASED_EXECUTION_SPEC.md)
- [STDF project specification](ZSTDF_SPEC.md)
- [Implementation plan](implementation_plan.md)
- [Parquet profiling guide](docs/parquet_profiling.md)

Build artifacts under `target` are generated locally and do not need to be
copied from the original computer.
