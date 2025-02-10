use actix_files::NamedFile;
use actix_web::{get, App, HttpResponse, HttpServer, Result, web, Error, HttpRequest};
use actix_web::middleware::Compress;
use actix_web::http::header;
use serde::{Serialize, Deserialize};
use std::{env, fs};
use std::path::{Path, PathBuf};
use std::time::Duration;
use percent_encoding::percent_decode_str;
use chrono::{DateTime, Local};
use std::process::Command;
use std::fs::OpenOptions;
use std::collections::BTreeMap;
use dav_server::DavHandler;
use dav_server::localfs::LocalFs;
use futures_util::StreamExt;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rand::{thread_rng, Rng};
use rand::distributions::Alphanumeric;

// 添加自定义序列化模块
mod ordered_map {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::collections::BTreeMap;

    pub fn serialize<S, K, V>(value: &BTreeMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        K: serde::Serialize + Ord,
        V: serde::Serialize,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(value.len()))?;
        for (k, v) in value {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }

    pub fn deserialize<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
    where
        D: Deserializer<'de>,
        K: serde::Deserialize<'de> + Ord,
        V: serde::Deserialize<'de>,
    {
        BTreeMap::deserialize(deserializer)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Config {
    ip: String,
    ipv6: String,
    port: u16,
    cwd: String,
    webdav: WebDAVConfig,  // 添加 WebDAV 配置
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct WebDAVConfig {
    enabled: bool,
    #[serde(with = "ordered_map")]  // 使用自定义序列化
    users: BTreeMap<String, UserConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UserConfig {
    password: String,
    permissions: String,  // "r" = read, "w" = write, "x" = execute
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
        let mut users = BTreeMap::new();
        users.insert("admin".to_string(), UserConfig {
            password: "admin".to_string(),
            permissions: "rwx".to_string(),
        });

        let config = Config {
            ip: "0.0.0.0".to_string(),
            ipv6: "::".to_string(),
            port: 8080,
            cwd: "data/www".to_string(),
            webdav: WebDAVConfig {
                enabled: false,
                users,
            },
        };

        let yaml_str = serde_yaml::to_string(&config)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        fs::write("data/config.yaml", yaml_str)?;
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
                
                // 检查是否为软链接
                let is_symlink = metadata.file_type().is_symlink();
                let is_dir = if is_symlink {
                    // 如果是软链接，获取目标文件的元数据
                    if let Ok(target_metadata) = fs::metadata(entry.path()) {
                        target_metadata.is_dir()
                    } else {
                        false  // 如果无法获取目标元数据，当作普通文件处理
                    }
                } else {
                    metadata.is_dir()
                };

                let size_string = if is_dir {
                    "目录".to_string()
                } else {
                    format_size(size)
                };
                
                let modified = metadata.modified().unwrap_or(std::time::SystemTime::now());
                let datetime: DateTime<Local> = modified.into();
                
                let file_entry = FileEntry {
                    name: name.clone(),
                    display_name: if is_symlink {
                        format!("{} ", name)
                    } else {
                        name.clone()
                    },
                    size_string,
                    modified_time: datetime.format("%Y-%m-%d %H:%M:%S").to_string(),
                    is_dir,
                    icon: if is_dir {
                        "📁".to_string()  // 文件夹图标
                    } else if is_symlink {
                        "🔗".to_string()  // 软链接图标
                    } else {
                        get_file_icon(&name).to_string()
                    },
                    preview_url: if is_previewable(&name) && !is_dir {
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
            icon: "📁".to_string(),
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

// 修改 WebDAV 处理函数
#[actix_web::route("/webdav/{tail:.*}", method="GET", method="HEAD", method="PUT", 
                   method="DELETE", method="COPY", method="MOVE", method="MKCOL", 
                   method="PROPFIND", method="PROPPATCH", method="LOCK", method="UNLOCK")]
async fn webdav_handler(
    req: HttpRequest,
    mut payload: web::Payload,
    config: web::Data<Config>,
) -> Result<HttpResponse, Error> {
    if !config.webdav.enabled {
        return Ok(HttpResponse::NotFound().body("WebDAV service is disabled"));
    }

    // 添加基本认证检查
    if let Some(auth) = req.headers().get(header::AUTHORIZATION) {
        let auth_str = auth.to_str().map_err(|_| {
            actix_web::error::ErrorUnauthorized("Invalid authorization header")
        })?;

        if auth_str.starts_with("Basic ") {
            let credentials = BASE64.decode(&auth_str[6..]).map_err(|_| {
                actix_web::error::ErrorUnauthorized("Invalid base64 in authorization")
            })?;

            let credentials_str = String::from_utf8(credentials).map_err(|_| {
                actix_web::error::ErrorUnauthorized("Invalid UTF-8 in authorization")
            })?;

            let parts: Vec<&str> = credentials_str.splitn(2, ':').collect();
            if parts.len() == 2 {
                let username = parts[0];
                let password = parts[1];

                if let Some(user_config) = config.webdav.users.get(username) {
                    if user_config.password != password {
                        return Ok(HttpResponse::Unauthorized()
                            .append_header((header::WWW_AUTHENTICATE, "Basic realm=\"WebDAV Server\""))
                            .body("Invalid password"));
                    }

                    // 检查权限
                    let method = req.method();
                    let need_write = matches!(method.as_str(), 
                        "PUT" | "DELETE" | "MKCOL" | "COPY" | "MOVE"
                    );

                    if need_write && !user_config.permissions.contains('w') {
                        return Ok(HttpResponse::Forbidden().body("Write permission required"));
                    }

                    if !user_config.permissions.contains('r') {
                        return Ok(HttpResponse::Forbidden().body("Read permission required"));
                    }
                } else {
                    return Ok(HttpResponse::Unauthorized()
                        .append_header((header::WWW_AUTHENTICATE, "Basic realm=\"WebDAV Server\""))
                        .body("Invalid username"));
                }
            }
        }
    } else {
        return Ok(HttpResponse::Unauthorized()
            .append_header((header::WWW_AUTHENTICATE, "Basic realm=\"WebDAV Server\""))
            .finish());
    }

    // 确保基础目录存在
    let base = PathBuf::from(&config.cwd);
    if !base.exists() {
        fs::create_dir_all(&base)?;
    }

    let handler = DavHandler::builder()
        .filesystem(LocalFs::new(&base, true, true, false))
        .strip_prefix("/webdav")
        .autoindex(true)
        .build_handler();

    let uri = req.uri().to_string();
    let mut dav_req = hyper::Request::builder()
        .method(req.method().clone())
        .uri(uri)
        .version(req.version());

    for (name, value) in req.headers() {
        dav_req = dav_req.header(name, value);
    }

    let body = if req.method() == hyper::Method::PUT {
        let (tx, body) = hyper::Body::channel();
        let mut tx = Some(tx);
        
        actix_web::rt::spawn(async move {
            while let Some(chunk) = payload.next().await {
                if let Ok(chunk) = chunk {
                    if let Some(tx) = tx.as_mut() {
                        if tx.send_data(chunk.into()).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        body
    } else {
        hyper::Body::empty()
    };

    let dav_req = dav_req.body(body)
        .unwrap_or_else(|_| hyper::Request::new(hyper::Body::empty()));

    let dav_resp = handler.handle(dav_req).await;
    let (parts, body) = dav_resp.into_parts();
    let mut builder = HttpResponse::build(parts.status);
    
    for (name, value) in parts.headers {
        if let Some(name) = name {
            builder.append_header((name, value));
        }
    }
    
    Ok(builder.streaming(body))
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
    println!("用法: webdisk [选项]");
    println!("\n选项:");
    println!("  -h, --help     显示帮助信息");
    println!("  -v, --version  显示版本信息");
    println!("  --webdav       WebDAV 配置");
    println!("\nWebDAV 配置:");
    println!("  --webdav true false          启用或禁用 WebDAV");
    println!("  --webdav add|del 用户名      添加或删除用户");
    println!("  --webdav 用户名:rwx 密码     设置权限和密码");
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

// 添加随机密码生成函数
fn generate_random_password() -> String {
    let mut rng = thread_rng();
    let password: String = (0..8)
        .map(|_| {
            let c = rng.sample(Alphanumeric) as char;
            if rng.gen_bool(0.5) {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect();
    password
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
            "--webdav" => {
                let mut config = Config::load()?;
                match args.get(2).map(|s| s.as_str()) {
                    Some("true") => {
                        config.webdav.enabled = true;
                        println!("WebDAV 已启用");
                    }
                    Some("false") => {
                        config.webdav.enabled = false;
                        println!("WebDAV 已禁用");
                    }
                    Some("add") => {
                        if let Some(username) = args.get(3) {
                            // 检查用户名是否包含权限设置
                            if username.contains(':') {
                                let parts: Vec<&str> = username.split(':').collect();
                                let username = parts[0];
                                let permissions = parts[1];
                                
                                // 验证权限字符串
                                if !permissions.chars().all(|c| "rwx".contains(c)) {
                                    println!("无效的权限字符串，只能包含 r、w、x");
                                    return Ok(());
                                }

                                // 检查用户是否已存在
                                if !config.webdav.users.contains_key(username) {
                                    if let Some(password) = args.get(4) {
                                        // 添加带权限和密码的用户
                                        config.webdav.users.insert(username.to_string(), UserConfig {
                                            password: password.to_string(),
                                            permissions: permissions.to_string(),
                                        });
                                        println!("已添加用户:");
                                        println!("- 用户名: {}", username);
                                        println!("- 密码: {}", password);
                                        println!("- 权限: {}", permissions);
                                    } else {
                                        // 添加带权限的用户，使用随机密码
                                        let random_password = generate_random_password();
                                        config.webdav.users.insert(username.to_string(), UserConfig {
                                            password: random_password.clone(),
                                            permissions: permissions.to_string(),
                                        });
                                        println!("已添加用户:");
                                        println!("- 用户名: {}", username);
                                        println!("- 密码: {}", random_password);
                                        println!("- 权限: {}", permissions);
                                    }
                                } else {
                                    println!("用户 {} 已存在", username);
                                }
                            } else {
                                // 原有的普通添加用户逻辑，使用随机密码
                                if !config.webdav.users.contains_key(username) {
                                    if let Some(password) = args.get(4) {
                                        config.webdav.users.insert(username.to_string(), UserConfig {
                                            password: password.to_string(),
                                            permissions: "r".to_string(),
                                        });
                                        println!("已添加用户:");
                                        println!("- 用户名: {}", username);
                                        println!("- 密码: {}", password);
                                        println!("- 权限: r");
                                    } else {
                                        let random_password = generate_random_password();
                                        config.webdav.users.insert(username.to_string(), UserConfig {
                                            password: random_password.clone(),
                                            permissions: "r".to_string(),
                                        });
                                        println!("已添加用户:");
                                        println!("- 用户名: {}", username);
                                        println!("- 密码: {}", random_password);
                                        println!("- 权限: r");
                                    }
                                } else {
                                    println!("用户 {} 已存在", username);
                                }
                            }
                        } else {
                            println!("请指定用户名");
                        }
                    }
                    Some("del") => {
                        if let Some(username) = args.get(3) {
                            if config.webdav.users.remove(username).is_some() {
                                println!("已删除用户 {}", username);
                            } else {
                                println!("用户 {} 不存在", username);
                            }
                        } else {
                            println!("请指定要删除的用户名");
                        }
                    }
                    Some(arg) => {
                        if let Some(username) = args.get(2) {
                            if arg.contains(':') {
                                // 设置用户权限
                                let parts: Vec<&str> = arg.split(':').collect();
                                let username = parts[0];
                                let permissions = parts[1];
                                
                                // 验证权限字符串
                                if !permissions.chars().all(|c| "rwx".contains(c)) {
                                    println!("无效的权限字符串，只能包含 r、w、x");
                                    return Ok(());
                                }

                                // 检查是否同时设置密码
                                if let Some(password) = args.get(3) {
                                    if let Some(user) = config.webdav.users.get_mut(username) {
                                        user.permissions = permissions.to_string();
                                        user.password = password.to_string();
                                        println!("已更新用户 {} 的权限为 {} 和密码", username, permissions);
                                    } else {
                                        // 如果用户不存在，创建新用户
                                        config.webdav.users.insert(username.to_string(), UserConfig {
                                            password: password.to_string(),
                                            permissions: permissions.to_string(),
                                        });
                                        println!("已创建用户 {}，设置权限为 {} 和密码", username, permissions);
                                    }
                                } else {
                                    // 只更新权限
                                    if let Some(user) = config.webdav.users.get_mut(username) {
                                        user.permissions = permissions.to_string();
                                        println!("已更新用户 {} 的权限为 {}", username, permissions);
                                    } else {
                                        println!("用户 {} 不存在", username);
                                    }
                                }
                            } else if let Some(password) = args.get(3) {
                                // 只设置密码
                                if let Some(user) = config.webdav.users.get_mut(username) {
                                    user.password = password.to_string();
                                    println!("已更新用户 {} 的密码", username);
                                } else {
                                    println!("用户 {} 不存在", username);
                                }
                            } else {
                                println!("无效的 WebDAV 命令");
                            }
                        } else {
                            println!("请指定用户名");
                        }
                    }
                    None => {
                        println!("WebDAV 状态: {}", if config.webdav.enabled { "已启用" } else { "已禁用" });
                        if !config.webdav.users.is_empty() {
                            println!("\n用户列表:");
                            for (username, user) in &config.webdav.users {
                                println!("- {}", username);
                                println!("  密码: {}", user.password);
                                println!("  权限: {}", user.permissions);
                            }
                        } else {
                            println!("未配置任何用户");
                        }
                    }
                }
                // 保存配置
                let yaml_str = serde_yaml::to_string(&config)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                fs::write("data/config.yaml", yaml_str)?;
                return Ok(());
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
    println!("- 目录: {}", config.cwd);

    // 添加 WebDAV 信息输出
    println!("\nWebDAV 信息:");
    println!("- 状态: {}", if config.webdav.enabled { "已启用" } else { "已禁用" });
    if config.webdav.enabled {
        if config.webdav.users.is_empty() {
            println!("- 用户: 未配置任何用户");
        } else {
            println!("- 已配置用户列表:");
            for (username, user_config) in &config.webdav.users {
                println!("  用户名: {}", username);
                println!("  密码: {}", user_config.password);
                println!("  权限: {}", user_config.permissions);
                println!();
            }
        }
    }

    println!("\n服务启动中...");
    
    let app_factory = {
        let config = config.clone();
        move || {
            let mut app = App::new()
                .wrap(Compress::default())
                .app_data(web::Data::new(config.clone()))
                .service(index);
            
            if config.webdav.enabled {
                app = app.service(webdav_handler);
            }
            
            app
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