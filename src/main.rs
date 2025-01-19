use actix_files::NamedFile;
use actix_web::{get, App, HttpResponse, HttpServer, Result, web};
use actix_web::middleware::Compress;
use serde::{Serialize, Deserialize};
use std::{env, fs};
use std::path::{Path, PathBuf};
use std::time::Duration;
use percent_encoding::percent_decode_str;
use chrono::{DateTime, Local};
use std::process::Command;
use std::fs::OpenOptions;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Config {
    ip: String,
    ipv6: String,  // 添加 IPv6 地址字段
    port: u16,
    cwd: String,
}

#[derive(Debug, Serialize)]
struct FileEntry {
    name: String,
    display_name: String,
    size_string: String,
    modified_time: String,
    is_dir: bool,
    icon: String,        // 添加图标字段
    preview_url: String, // 添加预览URL字段
}

impl Config {
    fn load() -> std::io::Result<Self> {
        let data_dir = Path::new("data");
        let config_path = data_dir.join("config.yaml");

        if !data_dir.exists() {
            fs::create_dir_all(data_dir)?;
        }

        if !config_path.exists() {
            Self::create_default_config()?;
        }
        
        let config_str = fs::read_to_string(&config_path)?;
        let config: Self = serde_yaml::from_str(&config_str)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        
        let cwd_path = Path::new(&config.cwd);
        if !cwd_path.exists() {
            fs::create_dir_all(cwd_path)?;
        }
        
        Ok(config)
    }

    // 添加创建默认配置的函数
    fn create_default_config() -> std::io::Result<()> {
        let default_config = r#"# 云溪起源网盘配置文件
ip: "0.0.0.0"     # IPv4 监听地址
ipv6: "::"        # IPv6 监听地址
port: 8080        # 监听的端口
cwd: "data/www"   # 文件存储目录"#;
        fs::write("data/config.yaml", default_config)?;
        println!("已创建默认配置文件");
        Ok(())
    }

    // 添加从指定路径加载配置的方法
    fn load_from(config_path: &Path) -> std::io::Result<Self> {
        if !config_path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                ConfigError("指定的配置文件不存在".to_string())
            ));
        }
        
        let config_str = fs::read_to_string(config_path)?;
        let config: Self = serde_yaml::from_str(&config_str)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        
        let cwd_path = Path::new(&config.cwd);
        if !cwd_path.exists() {
            fs::create_dir_all(cwd_path)?;
        }
        
        Ok(config)
    }
}


// 文件大小格式化
fn format_size(size: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = size as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

// 获取文件图标
fn get_file_icon(name: &str) -> &'static str {
    let extension = name.rsplit('.').next().unwrap_or("").to_lowercase();
    match extension.as_str() {
        // 镜像文件
        "iso" | "img" | "esd" | "wim" | "vhd" | "vmdk" => "💿",
        // 图片
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" => "🖼️",
        // 视频
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" => "🎥",
        // 音频
        "mp3" | "wav" | "ogg" | "m4a" | "flac" | "aac" => "🎵",
        // 文档
        "pdf" => "📕",
        "doc" | "docx" => "📘",
        "xls" | "xlsx" => "📗",
        "ppt" | "pptx" => "📙",
        "txt" | "md" | "log" => "📄",
        // 压缩文件
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" => "📦",
        // 代码文件
        "c" | "cpp" | "h" | "hpp" | "rs" | "go" | "py" | "js" | "html" | "css" | "java" => "📝",
        // 可执行文件
        "exe" | "msi" | "bat" | "sh" | "cmd" => "⚙️",
        // 配置文件
        "json" | "yaml" | "yml" | "toml" | "ini" | "conf" => "⚙️",
        // 字体文件
        "ttf" | "otf" | "woff" | "woff2" => "🔤",
        // 默认文件图标
        _ => "📄",
    }
}

// 判断文件是否可预览
fn is_previewable(name: &str) -> bool {
    let extension = name.rsplit('.').next().unwrap_or("").to_lowercase();
    matches!(extension.as_str(), 
        "jpg" | "jpeg" | "png" | "gif" | "webp" |
        "mp4" | "webm" |
        "mp3" | "wav" | "ogg"
    )
}

async fn get_directory_entries(path: &Path) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    
    if let Ok(read_dir) = fs::read_dir(path) {
        for entry in read_dir.flatten() {
            if let Ok(metadata) = entry.metadata() {
                let name = entry.file_name().to_string_lossy().to_string();
                let size = metadata.len();
                let is_dir = metadata.is_dir();
                let size_string = if is_dir {
                    "目录".to_string()
                } else {
                    format_size(size)
                };
                
                let modified = metadata.modified().unwrap_or(std::time::SystemTime::now());
                let datetime: DateTime<Local> = modified.into();
                
                let file_entry = FileEntry {
                    name: name.clone(),
                    display_name: name.clone(),
                    size_string,
                    modified_time: datetime.format("%Y-%m-%d %H:%M:%S").to_string(),
                    is_dir,
                    icon: get_file_icon(&name).to_string(),
                    preview_url: if is_previewable(&name) {
                        format!("./{}", name)
                    } else {
                        String::new()
                    },
                };

                if is_dir {
                    dirs.push(file_entry);
                } else {
                    files.push(file_entry);
                }
            }
        }
    }
    
    dirs.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    files.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    
    entries.extend(dirs);
    entries.extend(files);
    
    if path.parent().is_some() && path != Path::new(&"data/www") {
        entries.insert(0, FileEntry {
            name: "..".to_string(),
            display_name: "返回上级目录".to_string(),
            size_string: "".to_string(),
            modified_time: "".to_string(),
            is_dir: true,
            icon: "folder-up".to_string(),
            preview_url: String::new(),
        });
    }
    entries
}

#[get("/{path:.*}")]
async fn index(
    req: actix_web::HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let path = req.match_info().query("path").to_string();
    let full_path = PathBuf::from(&config.cwd).join(
        percent_decode_str(&path)
            .decode_utf8()
            .unwrap_or_default()
            .as_ref()
    );
    
    match (full_path.exists(), full_path.is_file()) {
        (false, _) => Ok(HttpResponse::NotFound().body("404 Not Found")),
        (true, true) => Ok(NamedFile::open(&full_path)?.into_response(&req)),
        (true, false) => {
            let entries = get_directory_entries(&full_path).await;
            
            let mut context = tera::Context::new();
            context.insert("current_path", &path);
            context.insert("entries", &entries);
            
            let rendered = tera::Tera::one_off(TEMPLATE, &context, false)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            
            Ok(HttpResponse::Ok()
                .content_type("text/html; charset=utf-8")
                .body(rendered))
        }
    }
}

const TEMPLATE: &str = r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>文件索引</title>
    <link rel="icon" href="/favicon.ico"/>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
            margin: 20px;
            background-color: #f8f9fa;
        }
        .entry {
            display: flex;
            align-items: center;
            padding: 15px;
            margin: 5px 0;
            background-color: white;
            border-radius: 8px;
            box-shadow: 0 1px 3px rgba(0,0,0,0.1);
        }
        .entry:hover {
            background-color: #f8f9fa;
        }
        .info-group {
            display: flex;
            align-items: center;
            gap: 20px;
            margin-left: auto;
        }
        a {
            text-decoration: none;
            color: inherit;
        }
        a:hover {
            text-decoration: underline;
        }
        h1 {
            color: #333;
            border-bottom: 2px solid #ddd;
            padding-bottom: 10px;
            font-size: 1.5em;
            word-break: break-all;
        }
        .name-column {
            flex: 2;
            min-width: 0;
            overflow: visible;
            text-overflow: ellipsis;
            white-space: normal;
            word-break: break-all;
        }
        .size-column {
            flex: 0.8;
            text-align: right;
            min-width: 80px;
        }
        .date-column {
            flex: 1.2;
            text-align: right;
            white-space: nowrap;
            min-width: 150px;
        }
        .preview-container {
            display: none;
            margin: 8px 0 8px 32px;
            vertical-align: middle;
        }
        .preview-container img {
            max-width: 160px;
            max-height: 90px;
            object-fit: contain;
            border-radius: 4px;
            display: block;
        }
        .preview-container video {
            max-width: 160px;
            max-height: 90px;
            object-fit: contain;
            border-radius: 4px;
            display: block;
        }
        .preview-container audio {
            width: 320px;
            height: 32px;
            display: block;
        }
        .file-icon {
            margin-right: 8px;
            font-size: 1.2em;
            display: inline-block;
            width: 32px;
            text-align: center;
        }
        .download-btn {
            background-color: #4CAF50;
            color: white;
            padding: 4px 8px;
            border-radius: 4px;
            font-size: 0.8em;
            text-decoration: none;
            display: inline-block;
            margin-right: 10px;
            min-width: 50px;
            text-align: center;
            white-space: nowrap;
        }
        .preview-btn {
            background-color: #2196F3;
            color: white;
            padding: 4px 8px;
            border-radius: 4px;
            font-size: 0.8em;
            cursor: pointer;
            margin-right: 10px;
            min-width: 50px;
            text-align: center;
            white-space: nowrap;
        }
        .footer {
            position: fixed;
            bottom: 0;
            left: 0;
            right: 0;
            width: 100%;
            text-align: center;
            padding: 20px 0;
            background-color: #f8f9fa;
            border-top: 1px solid #eee;
        }
        .footer a {
            color: #666;
            text-decoration: none;
            font-size: 14px;
            display: block;
            margin: 0 auto;
        }
        .footer p {
            margin: 5px 0;
            color: #999;
            font-size: 12px;
        }
        body {
            margin-bottom: 100px;
        }
        @media (max-width: 768px) {
            body {
                margin: 10px;
            }
            .entry {
                flex-direction: column;
                align-items: flex-start;
                gap: 8px;
                padding: 12px;
            }
            .name-column {
                flex: 1;
                width: 100%;
                margin-bottom: 4px;
            }
            .info-group {
                width: 100%;
                justify-content: flex-start;
                flex-wrap: wrap;
                gap: 10px;
            }
            .size-column {
                min-width: auto;
                order: 2;
            }
            .date-column {
                min-width: auto;
                width: 100%;
                text-align: left;
                order: 3;
            }
            .download-btn {
                order: 1;
                margin-right: 0;
            }
            h1 {
                font-size: 1.2em;
            }
        }
    </style>
</head>
<body>
    <h1>目录: /{{current_path}}</h1>
    {% for entry in entries %}
    <div class="entry">
        <div class="name-column">
            {% if entry.is_dir %}
            <a href="./{{entry.name}}/" class="directory">📁 {{entry.name}}/</a>
            {% else %}
            <a href="./{{entry.name}}">
                <span class="file-icon" id="icon-{{entry.name}}">{{entry.icon}}</span>
                <span class="preview-container" id="preview-{{entry.name}}"></span>
                {{entry.display_name}}
            </a>
            {% endif %}
        </div>
        <div class="info-group">
            {% if not entry.is_dir %}
                {% if entry.preview_url != "" %}
                <span class="preview-btn" onclick="togglePreview('{{entry.preview_url}}', '{{entry.display_name}}')">预览</span>
                {% endif %}
                <a href="./{{entry.name}}" class="download-btn" download="{{entry.display_name}}">下载</a>
                <div class="size-column">{{entry.size_string}}</div>
            {% endif %}
            <div class="date-column">{{entry.modified_time}}</div>
        </div>
    </div>
    {% endfor %}

    <div id="preview-modal" class="preview-modal" onclick="this.style.display='none'">
        <div class="preview-content" id="preview-content" onclick="event.stopPropagation()"></div>
    </div>
        <footer class="footer">
        <a href="https://yxyos.cn" target="_blank">
            <p>©2025 云溪起源</p>
        </a>
    </footer>
    <script>
    function togglePreview(url, name) {
        const previewContainer = document.getElementById(`preview-${name}`);
        const icon = document.getElementById(`icon-${name}`);
        const ext = name.split('.').pop().toLowerCase();
        
        if (previewContainer.style.display === 'block') {
            previewContainer.style.display = 'none';
            icon.style.display = 'inline-block';
            previewContainer.innerHTML = '';
            return;
        }

        icon.style.display = 'none';
        previewContainer.style.display = 'block';
        
        if (['jpg', 'jpeg', 'png', 'gif', 'webp'].includes(ext)) {
            previewContainer.innerHTML = `<img src="${url}" alt="${name}">`;
        } else if (['mp4', 'webm'].includes(ext)) {
            previewContainer.innerHTML = `<video src="${url}" controls></video>`;
        } else if (['mp3', 'wav', 'ogg'].includes(ext)) {
            previewContainer.innerHTML = `<audio src="${url}" controls></audio>`;
        }
    }
    </script>
</body>
</html>
"#;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS", "yxyos");
const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

fn print_version() {
    println!("云溪起源网盘 v{}", VERSION);
    println!("作者: {}", AUTHORS);
    println!("描述: {}", DESCRIPTION);
}

fn print_help() {
    println!("云溪起源网盘 v{}", VERSION);
    println!("用法: yunxi-webdisk [选项]");
    println!("\n选项:");
    println!("  --host ip <地址>       设置IPv4监听地址");
    println!("  --host ipv6 <地址>     设置IPv6监听地址");
    println!("  --host port <端口>     设置监听端口");
    println!("  --host cwd <目录>      设置文件存储目录");
    println!("  --config <文件路径>    使用指定的配置文件");
    println!("  --config default       重建默认配置文件");
    println!("  start                  后台启动服务");
    println!("  stop                   停止服务");
    println!("  -h, --help            显示帮助信息");
    println!("  -v, --version         显示版本信息");
}

// 修改错误类型
#[derive(Debug)]
struct ConfigError(String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ConfigError {}

fn is_valid_ip(value: &str) -> bool {
    if !value.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return false;
    }
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|part| part.parse::<u8>().is_ok())  // 直接检查解析结果
}

fn is_valid_domain(value: &str) -> bool {
    // 简单的域名验证规则
    if value.is_empty() || value.len() > 253 {
        return false;
    }
    
    // 只允许字母、数字、点和连字符
    if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return false;
    }
    
    // 不能以点或连字符开始或结束
    if value.starts_with('.') || value.starts_with('-') || 
       value.ends_with('.') || value.ends_with('-') {
        return false;
    }
    
    // 检查每个部分
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() < 2 {  // 至少需要有一个顶级域名
        return false;
    }
    
    // 检查每个部分的长度和格式
    parts.iter().all(|part| {
        !part.is_empty() && part.len() <= 63 && 
        !part.starts_with('-') && !part.ends_with('-')
    })
}

fn is_valid_ipv6(value: &str) -> bool {
    // 特殊情况处理
    if value == "::" || value == "::1" {
        return true;
    }
    
    // 检查基本格式
    if !value.chars().all(|c| c.is_ascii_hexdigit() || c == ':') {
        return false;
    }
    
    let parts: Vec<&str> = value.split(':').collect();
    
    // IPv6 地址最多可以有 8 个部分
    // 如果有 :: 缩写，parts 的长度可能小于 8
    if parts.len() > 8 {
        return false;
    }
    
    // 检查每个部分
    let mut has_empty = false;
    for part in parts {
        if part.is_empty() {
            if has_empty {
                // 只允许一个 :: 缩写
                return false;
            }
            has_empty = true;
            continue;
        }
        
        // 每个部分最多 4 个十六进制数字
        if part.len() > 4 {
            return false;
        }
        
        // 检查是否都是有效的十六进制数字
        if !part.chars().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
    }
    
    true
}

fn update_config(key: &str, value: &str) -> std::io::Result<()> {
    let config_path = Path::new("data/config.yaml");
    let config_str = fs::read_to_string(&config_path)?;
    let mut config: serde_yaml::Value = serde_yaml::from_str(&config_str)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    match key {
        "ip" => {
            if !is_valid_ip(value) && !is_valid_domain(value) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    ConfigError("必须是有效的IPv4地址（如 127.0.0.1）或域名（如 example.com）".to_string())
                ));
            }
            config["ip"] = serde_yaml::Value::String(value.to_string());
        }
        "ipv6" => {
            if value == "no" {
                config["ipv6"] = serde_yaml::Value::String("".to_string());
            } else if !is_valid_ipv6(value) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    ConfigError("必须是有效的IPv6地址（如 ::1 或 2001:db8::1）或 'no' 以禁用 IPv6".to_string())
                ));
            } else {
                config["ipv6"] = serde_yaml::Value::String(value.to_string());
            }
        }
        "port" => {
            match value.parse::<u16>() {
                Ok(port) if port > 0 => {
                    config["port"] = serde_yaml::Value::Number(serde_yaml::Number::from(port));
                }
                _ => return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    ConfigError("端口必须是1-65535之间的数字".to_string())
                ))
            }
        }
        "cwd" => {
            let path = Path::new(value);
            if !path.is_absolute() && !value.starts_with("./") && !value.starts_with("../") {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    ConfigError("路径必须是绝对路径或以 ./ 或 ../ 开头的相对路径".to_string())
                ));
            }
            config["cwd"] = serde_yaml::Value::String(value.to_string());
        }
        _ => return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            ConfigError("无效的配置项，只能是 ip、port 或 cwd".to_string())
        ))
    }

    let new_config = serde_yaml::to_string(&config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&config_path, new_config)?;
    println!("已更新配置: {} = {}", key, value);
    Ok(())
}

fn write_pid() -> std::io::Result<()> {
    let pid = std::process::id().to_string();
    fs::write("data/yunxi-webdisk.pid", pid)?;
    Ok(())
}

fn read_pid() -> std::io::Result<u32> {
    let pid_str = fs::read_to_string("data/yunxi-webdisk.pid")?;
    pid_str.trim().parse::<u32>()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid PID"))
}

#[cfg(target_family = "unix")]
fn stop_process(pid: u32) -> std::io::Result<()> {
    unsafe {
        // 首先尝试优雅停止 (SIGTERM)
        if libc::kill(pid as i32, libc::SIGTERM) == 0 {
            // 等待最多3秒
            for _ in 0..30 {
                if libc::kill(pid as i32, 0) != 0 {
                    // 进程已经停止
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            // 如果进程还在运行，强制结束 (SIGKILL)
            if libc::kill(pid as i32, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
        } else {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(target_family = "windows")]
fn stop_process(pid: u32) -> std::io::Result<()> {
    Command::new("taskkill")
        .args(&["/PID", &pid.to_string(), "/F"])
        .output()?;
    Ok(())
}

// 修改错误处理函数，使用引用而不是获取所有权
fn format_error(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::AddrNotAvailable => {
            "无法绑定到指定地址，请检查IP地址是否正确或端口是否被占用".to_string()
        }
        std::io::ErrorKind::AddrInUse => {
            "端口已被占用".to_string()
        }
        std::io::ErrorKind::PermissionDenied => {
            "权限不足，请检查端口号是否小于1024或是否有管理员权限".to_string()
        }
        _ => {
            format!("启动失败: {}", e)
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() > 1 {
        match args[1].as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "-v" | "--version" => {
                print_version();
                return Ok(());
            }
            "--host" => {
                if args.len() == 4 {
                    if let Err(e) = update_config(&args[2], &args[3]) {
                        eprintln!("{}", e.get_ref().unwrap().to_string());
                        std::process::exit(1);
                    }
                    return Ok(());
                } else {
                    println!("无效的命令格式，使用 -h 或 --help 查看帮助");
                    return Ok(());
                }
            }
            "--config" => {
                if args.len() == 3 {
                    if args[2] == "default" {
                        if Path::new("data/config.yaml").exists() {
                            println!("警告: 配置文件已存在，将被覆盖");
                            println!("按回车键继续，或 Ctrl+C 取消");
                            let mut input = String::new();
                            std::io::stdin().read_line(&mut input)?;
                        }
                        Config::create_default_config()?;
                    } else {
                        // 使用指定的配置文件
                        let config_path = Path::new(&args[2]);
                        match Config::load_from(config_path) {
                            Ok(_) => {
                                println!("已加载配置文件: {}", args[2]);
                                // 将配置文件路径保存到环境变量中
                                env::set_var("YUNXI_CONFIG", &args[2]);
                            }
                            Err(e) => {
                                eprintln!("加载配置文件失败: {}", e);
                                std::process::exit(1);
                            }
                        }
                    }
                    return Ok(());
                } else {
                    println!("无效的命令格式，使用 -h 或 --help 查看帮助");
                    return Ok(());
                }
            }
            "start" => {
                // 检查是否已经在运行
                if let Ok(_) = read_pid() {
                    println!("服务已经在运行中");
                    return Ok(());
                }

                // 启动后台进程
                let exe = env::current_exe()?;
                Command::new(exe)
                    .arg("run")
                    .stdin(std::process::Stdio::null())
                    .stdout(OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("data/yunxi-webdisk.log")?)
                    .stderr(OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("data/yunxi-webdisk.log")?)
                    .spawn()?;
                println!("服务已在后台启动");
                return Ok(());
            }
            "stop" => {
                if let Ok(pid) = read_pid() {
                    match stop_process(pid) {
                        Ok(_) => {
                            if let Err(e) = fs::remove_file("data/yunxi-webdisk.pid") {
                                println!("警告: 无法删除PID文件: {}", e);
                            }
                            println!("服务已停止");
                        }
                        Err(e) => {
                            println!("停止服务失败: {}", e);
                            // 如果进程已经不存在，仍然删除PID文件
                            #[cfg(target_family = "unix")]
                            let process_not_exists = e.raw_os_error() == Some(libc::ESRCH);
                            #[cfg(target_family = "windows")]
                            let process_not_exists = e.kind() == std::io::ErrorKind::NotFound;

                            if process_not_exists {
                                if let Err(e) = fs::remove_file("data/yunxi-webdisk.pid") {
                                    println!("警告: 无法删除PID文件: {}", e);
                                }
                                println!("进程已经不存在，已清理PID文件");
                            }
                        }
                    }
                } else {
                    println!("服务未运行");
                }
                return Ok(());
            }
            "run" => {
                // 内部命令，用于实际运行服务
                write_pid()?;
            }
            _ => {
                println!("未知命令，使用 -h 或 --help 查看帮助");
                return Ok(());
            }
        }
    }

    let config = if let Ok(config_path) = env::var("YUNXI_CONFIG") {
        Config::load_from(Path::new(&config_path))?
    } else {
        Config::load()?
    };

    let bind_addr_v4 = format!("{}:{}", config.ip, config.port);
    let ipv6_bind = if config.ipv6.starts_with('[') {
        format!("{}:{}", config.ipv6, config.port)
    } else {
        format!("{}:{}", config.ipv6, config.port)
    };
    let has_ipv6 = !config.ipv6.is_empty();
    
    println!("\n云溪起源网盘 v{}", VERSION);
    println!("作者: {}", AUTHORS);
    println!("描述: {}\n", DESCRIPTION);
    
    println!("系统信息:");
    println!("- PID: {}", std::process::id());
    println!("- IPv4: http://{}", bind_addr_v4);
    if has_ipv6 {
        let display_ipv6 = if config.ipv6.starts_with('[') {
            config.ipv6.to_string()
        } else {
            format!("[{}]", config.ipv6)
        };
        println!("- IPv6: http://{}:{}", display_ipv6, config.port);
    }
    println!("- 目录: {}\n", config.cwd);
    
    println!("服务启动中...");
    
    let app_factory = {
        let config = config.clone();
        move || {
            App::new()
                .wrap(Compress::default())
                .app_data(web::Data::new(config.clone()))
                .service(index)
        }
    };
    
    // 创建基本的服务器配置
    let make_server = || {
        HttpServer::new(app_factory.clone())
            .workers(num_cpus::get())
            .backlog(1024)
            .keep_alive(Duration::from_secs(30))
    };

    // 尝试绑定 IPv4
    let server = match make_server().bind(&bind_addr_v4) {
        Ok(ipv4_server) => {
            if has_ipv6 {
                match ipv4_server.bind(&ipv6_bind) {
                    Ok(dual_server) => {
                        println!("服务器启动成功");
                        dual_server
                    }
                    Err(e) => {
                        println!("服务器启动成功（仅 IPv4）");
                        println!("IPv6 绑定失败: {}", format_error(&e));
                        make_server().bind(&bind_addr_v4)?
                    }
                }
            } else {
                println!("服务器启动成功");
                ipv4_server
            }
        }
        Err(e) => {
            eprintln!("IPv4 绑定失败: {}", format_error(&e));
            if has_ipv6 {
                match make_server().bind(&ipv6_bind) {
                    Ok(ipv6_server) => {
                        println!("服务器启动成功（仅 IPv6）");
                        ipv6_server
                    }
                    Err(e2) => {
                        eprintln!("IPv6 绑定失败: {}", format_error(&e2));
                        return Err(e);
                    }
                }
            } else {
                return Err(e);
            }
        }
    };

    // 启动服务器
    if let Err(e) = server.run().await {
        eprintln!("{}", format_error(&e));
        std::process::exit(1);
    }

    Ok(())
}