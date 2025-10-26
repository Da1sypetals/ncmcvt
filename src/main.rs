use clap::Parser;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
mod ncm;

/// 从网易云音乐的 .ncm 文件格式中解密音乐文件。
/// 默认输出为同名 .mp3 / .flac 文件。
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 一个或多个 .ncm 文件的路径
    #[arg(required = true, name = "FILES")]
    files: Vec<PathBuf>,

    /// 输出目录（如果没有指定，默认输出到 ~/Instrumental）
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// 如果输出文件已存在则跳过（如果没有指定，默认覆盖）
    #[arg(short, long)]
    skip: bool,
}

fn main() {
    let args = Args::parse();

    for path in &args.files {
        if path.is_dir() {
            // 如果是目录，则遍历目录
            for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                if entry.path().is_file()
                    && entry.path().extension().map_or(false, |ext| ext == "ncm")
                {
                    process_file(entry.path(), &args.output, args.skip);
                }
            }
        } else if path.is_file() {
            // 如果是文件
            process_file(path, &args.output, args.skip);
        } else {
            eprintln!("错误: 找不到文件或目录 '{}'", path.display());
        }
    }
}

/// 处理单个 NCM 文件。
fn process_file(input_path: &Path, output_dir: &Option<PathBuf>, skip: bool) {
    println!("正在处理: {}", input_path.display());

    let output_path = match output_dir {
        Some(dir) => {
            let file_name = input_path.file_stem().unwrap_or_else(|| {
                // 如果没有文件名，则使用默认名称
                std::ffi::OsStr::new("unnamed_file")
            });
            Some(dir.join(file_name))
        }
        None => None, // dump 函数将处理 None 的情况
    };

    match ncm::decrypt_and_dump(input_path, output_path.as_deref(), skip) {
        Ok(final_path) => println!("成功解密到: \"{}\"", final_path.display()),
        Err(e) => eprintln!("处理 \"{}\" 时出错: {}", input_path.display(), e),
    }
}
