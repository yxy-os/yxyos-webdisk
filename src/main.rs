use actix_files::NamedFile;
use actix_web::{get, App, HttpResponse, HttpServer, Result, web};
use actix_web::middleware::Compress;
use serde::{Serialize, Deserialize};
use std::{env, fs};
use std::path::{Path, PathBuf};
use std::time::Duration;
use percent_encoding::percent_decode_str;
use chrono::{DateTime, Local};

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Config {
    ip: String,
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
            let default_config = r#"# 云溪起源网盘配置文件
ip: "0.0.0.0"    # 监听的 IP 地址
port: 8080       # 监听的端口
cwd: "data/www"  # 文件存储目录"#;
            fs::write(&config_path, default_config)?;
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 处理命令行参数，使用 print_version 函数
    if let Some(arg) = env::args().nth(1) {
        if arg == "-v" || arg == "--version" {
            print_version();  // 使用 print_version 函数
            return Ok(());
        }
    }

    let config = Config::load()?;
    let bind_addr = format!("{}:{}", config.ip, config.port);
    
    // 使用所有常量显示启动信息
    println!("\n云溪起源网盘 v{}", VERSION);
    println!("作者: {}", AUTHORS);
    println!("描述: {}\n", DESCRIPTION);
    
    println!("系统信息:");
    println!("- PID: {}", std::process::id());
    println!("- 地址: http://{}", bind_addr);
    println!("- 目录: {}\n", config.cwd);
    
    println!("服务启动中...");
    
    HttpServer::new(move || {
        App::new()
            .wrap(Compress::default())
            .app_data(web::Data::new(config.clone()))
            .service(index)
    })
    .workers(num_cpus::get())
    .backlog(1024)
    .keep_alive(Duration::from_secs(30))
    .bind(&bind_addr)?
    .run()
    .await
}