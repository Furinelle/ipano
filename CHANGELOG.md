# 更新日志

本项目的所有重要变更都记录在此文件。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/),版本遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [0.1.0] - 2026-06-12

地基 MVP(设计文档 P0–P1)。

### 新增

- **CLI 框架**:`ipano [IP]`,支持 `-4`/`-6`/`--json`/`--no-color`/`--timeout` 参数(基于 clap)
- **双查询模式**:无参数查本机出口 IP(IPv4/IPv6,多端点取众数),带参数查指定 IP
- **核心抽象**:`Source` trait + `run_all` 并发调度,单源失败不影响整体
- **数据源**:ip-api、ipinfo、ip.sb 三个免 key 基础源(每源拆分纯解析层 + 抓取层)
- **聚合**:`merge()` 按源优先级(ipinfo > ipsb > ipapi)去重合并基础字段,记录各源成功/失败状态
- **渲染**:彩色终端报告(comfy-table + owo-colors)与机器可读 JSON 双输出
- **测试**:19 个单元/集成测试(解析层纯函数 + httpmock 模拟抓取,不依赖真实网络)

### 说明

- 编译为单静态二进制(rustls,无系统 OpenSSL 依赖)
- `SourceData` 已预留 `is_vpn`/`is_tor`/`ip_type` 等字段,供后续 ping0 及欺诈库源填充

[0.1.0]: https://github.com/Furinelle/ipano/releases/tag/v0.1.0
