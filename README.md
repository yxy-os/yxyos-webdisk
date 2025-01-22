# 云溪起源网盘

> Gitee: [https://gitee.com/yxyos/yxyos-files](https://gitee.com/yxyos/yxyos-files)<br>
> Github: [https://github.com/yxy-os/yxyos-files](https://github.com/yxy-os/yxyos-files)

一个轻量级的文件服务器，使用 Rust + Actix-web 开发。

## 特性

- 🚀 高性能：使用 Rust 语言开发，性能优异
- 📁 文件浏览：支持目录浏览和文件下载
- 🖼️ 文件预览：支持图片、视频、音频等文件在线预览
- 💡 智能图标：根据文件类型显示不同图标
- 📱 响应式设计：支持移动端访问
- 🔧 简单配置：通过 YAML 文件轻松配置
- 🗜️ 压缩传输：支持 HTTP 压缩

## 快速开始

### 下载和运行

1. 从 Release 页面下载对应平台的可执行文件
2. 运行程序：
   ```bash
   ./webdisk
   ```
3. 访问 `http://localhost:8080` 即可使用

### 配置说明

首次运行时会在 `data` 目录下自动创建 `config.yaml` 配置文件：

```yaml
# 云溪起源网盘配置文件
ip: "0.0.0.0"    # 监听的 IP 地址
ipv6: '::'`
port: 8080       # 监听的端口
cwd: "data/www"  # 文件存储目录
```

### 命令行参数

- `-v` 或 `--version`: 显示版本信息

## 支持的文件预览

### 图片格式
- jpg, jpeg
- png
- gif
- webp
- svg

### 视频格式
- mp4
- webm
- mkv

### 音频格式
- mp3
- wav
- ogg
- flac

### 文档格式
- pdf
- txt
- md

## 开发相关

### 环境要求

- Rust 1.70+
- Cargo

### 构建

```bash
cargo build --release
```

### 运行开发版本

```bash
cargo run
```

### 目录结构

```
.
├── src/            # 源代码目录
└── data/           # 数据目录
    ├── www/       # 文件存储目录
    └── config.yaml # 配置文件
```

## 技术栈

- 后端：Rust + Actix-web
- 前端：HTML + CSS + JavaScript
- 存储：本地文件系统
- 配置：YAML

## 作者

云溪起源团队

## 反馈与贡献

- 提交问题：请使用 GitHub Issues
- 贡献代码：欢迎提交 Pull Request
- 功能建议：可以在 Discussions 中讨论

©2025 云溪起源。保留所有权利。
