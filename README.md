# Halcon License Fetcher

[![Build](https://github.com/JokinYang/halcon-license-fetcher/actions/workflows/release.yml/badge.svg)](https://github.com/JokinYang/halcon-license-fetcher/actions/workflows/release.yml)
[**下载最新版本**](https://github.com/JokinYang/halcon-license-fetcher/releases/download/latest/halcon-license-fetcher.exe)

自动从 GitHub 下载并安装 MVTec 产品（HALCON / HDevelop / DeepLearningTool）许可证的工具。

许可证来源：

- **eval**（默认）：[starain2000/HalconEvalLicenses](https://github.com/starain2000/HalconEvalLicenses) — zip 归档格式
- **community**：[lovelyyoshino/Halcon_licenses](https://github.com/lovelyyoshino/Halcon_licenses) — 目录格式

通过 `--source` 选项切换源。两个源自动适配不同的文件组织格式。

## 功能

- **全自动检测** — 自动发现所有已安装的 MVTec 产品及版本
- **多产品支持** — HALCON / HDevelop / DeepLearningTool 一站式处理
- **多 License 合并** — 自动合并 Support + Eval + DL 许可证到同一个 `license.dat`
- **Windows 服务** — 一键注册为系统服务，开机自启，每 7 天自动更新

## 快速开始

```powershell
# 手动运行一次
.\halcon-license-fetcher.exe

# 注册为 Windows 服务（管理员）
.\halcon-license-fetcher.exe service install
```

## 命令参考

```
halcon-license-fetcher [OPTIONS] [COMMAND]
```

### 命令

| 命令 | 说明 |
|------|------|
| *(无)* | 手动运行一次，扫描并安装 license |
| `list-months` | 列出所有可用月份 |
| `service install` | 注册为 Windows 服务 |
| `service remove` | 移除 Windows 服务 |
| `help` | 查看帮助 |

### 选项

| 选项 | 说明 |
|------|------|
| `--halcon-root <PATH>` | 手动指定安装目录 |
| `--month <YYYY.MM>` | 指定月份（默认：最新） |
| `--source <SOURCE>` | License 源: `eval`（默认）或 `community` |
| `--dry-run` | 仅预览，不写入 |
| `--force` | 强制覆盖备份 |

### service 子命令

```
halcon-license-fetcher service install [OPTIONS]
      --days <DAYS>        每月执行日（1-28，逗号分隔，如 1,15）
      --interval <DAYS>    间隔天数（如 7）
      --source <SOURCE>    License 源（默认 eval）
      --nssm-path <PATH>   手动指定 nssm.exe 路径

halcon-license-fetcher service remove
      --nssm-path <PATH>   手动指定 nssm.exe 路径
```

## 产品类型与 License 匹配

| 安装目录示例 | 类型 | 匹配的 License |
|-------------|------|---------------|
| `HALCON-24.11` | Steady | `*halcon24.11_steady_*` |
| `HALCON-26.05-Progress` | Progress | `*halcon_progress_*` + HDevelop |
| `HALCON-24.11-Progress-Steady` | Steady+DL | `*halcon24.11_steady-dl_*` + steady + progress |
| `DeepLearningTool` | DLT | `*eval_dlt_*` + HDevelop |

## 服务管理

安装后可使用标准 Windows 命令管理：

```powershell
sc query HalconLicenseFetcher    # 查看状态
sc stop  HalconLicenseFetcher    # 停止
sc start HalconLicenseFetcher    # 启动
```

服务日志位于 exe 同目录的 `service_stdout.log` 和 `service_stderr.log`。

## 构建

```bash
# 需要将 nssm.exe 放入 assets/ 目录
# 下载: https://nssm.cc/release/nssm-2.24.zip → 解压 win64/nssm.exe

cargo build --release
```

## 许可

MIT
