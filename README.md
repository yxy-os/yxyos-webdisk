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
- 📂 WebDAV：支持 WebDAV 协议，可挂载为网络驱动器

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
ipv6: '::'       # IPv6 地址
port: 8080       # 监听的端口
cwd: "data/www"  # 文件存储目录

# WebDAV 配置
webdav:
  enabled: true  # 是否启用 WebDAV
  users:         # WebDAV 用户配置
    admin:       # 用户名
      password: "admin"     # 密码
      permissions: "rwx"    # 权限：r=读取，w=写入，x=执行
```

### WebDAV 使用说明

#### 1. 配置 WebDAV

在 `config.yaml` 中启用 WebDAV 并配置用户：

```yaml
webdav:
  enabled: true
  users:
    admin:
      password: "your_password"
      permissions: "rwx"    # 完全访问权限
    readonly:
      password: "read123"
      permissions: "r"      # 只读权限
```

#### 2. API 调用

WebDAV 支持以下 HTTP 方法：

- `PROPFIND`: 获取文件/目录信息
- `GET`: 下载文件
- `PUT`: 上传文件
- `DELETE`: 删除文件
- `MKCOL`: 创建目录
- `COPY`: 复制文件
- `MOVE`: 移动文件

示例：
```bash
# 列出目录内容
curl -X PROPFIND -u admin:password http://localhost:8080/webdav/

# 上传文件
curl -T file.txt -u admin:password http://localhost:8080/webdav/file.txt

# 下载文件
curl -u admin:password http://localhost:8080/webdav/file.txt

# 创建目录
curl -X MKCOL -u admin:password http://localhost:8080/webdav/newdir

# 删除文件
curl -X DELETE -u admin:password http://localhost:8080/webdav/file.txt
```

### 命令行参数

- `-h, --help`: 显示帮助信息
- `-v, --version`: 显示版本信息
- `--host`: 修改服务器配置
  - `--host ip <地址>`: 设置 IPv4 监听地址
  - `--host ipv6 <地址>`: 设置 IPv6 监听地址
  - `--host port <端口>`: 设置监听端口
  - `--host cwd <目录>`: 设置文件存储目录
- `--config`: 配置文件操作
  - `--config default`: 重建默认配置文件
  - `--config <文件路径>`: 使用指定的配置文件
- `start`: 后台启动服务
- `stop`: 停止服务
- `--webdav`: WebDAV 配置
  - `--webdav true|false`: 启用或禁用 WebDAV
  - `--webdav add|del 用户名`: 添加或删除用户
  - `--webdav 用户名:rwx 密码`: 设置用户权限和密码

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
