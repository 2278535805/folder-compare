use anyhow::{Context, Result};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use md5;
use rayon::prelude::*;
use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::exit, 
    sync::{Arc, Mutex},
};
use walkdir::WalkDir;

fn calculate_md5(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .with_context(|| format!("无法打开文件: {}", path.display()))?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .with_context(|| format!("无法读取文件: {}", path.display()))?;
    let digest = md5::compute(&buffer);
    Ok(format!("{:x}", digest))
}

fn get_md5_dict(dir: &Path) -> Result<HashMap<String, Vec<PathBuf>>> {
    let paths: Vec<_> = WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect();

    let total = paths.len() as u64;
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {pos}/{len} ({eta_precise})")
            .unwrap()
            .progress_chars("=>-"),
    );
    pb.set_message(format!("计算 {}", dir.display()));

    let map = Arc::new(Mutex::new(HashMap::<String, Vec<PathBuf>>::new()));
    
    paths.par_iter().for_each(|path| {
        match calculate_md5(path) {
            Ok(hash) => {
                let mut map_lock = map.lock().unwrap();
                map_lock.entry(hash).or_default().push(path.clone());
            }
            Err(e) => eprintln!("计算文件 {} MD5 时出错: {}", path.display(), e),
        }
        pb.inc(1);
    });

    pb.finish_with_message(format!("{} 完成", dir.display().to_string().green()));
    
    let result_map = Arc::try_unwrap(map)
        .unwrap()
        .into_inner()
        .unwrap();
    
    Ok(result_map)
}

fn compare_folders(dir_a: &Path, dir_b: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let a_map = get_md5_dict(dir_a)?;
    println!("{}", "文件夹 A 计算完毕".green());
    let b_map = get_md5_dict(dir_b)?;
    println!("{}", "文件夹 B 计算完毕".green());

    let mut b_duplicates = Vec::new();
    let mut b_unique = Vec::new();

    // A 中的文件
    for (md5, a_paths) in &a_map {
        if let Some(b_paths) = b_map.get(md5) {
            b_duplicates.extend(b_paths.clone());
        } else {
            println!("{} {}", format!("在 A 独有 (MD5 = {})", md5).red(), "");
            for p in a_paths {
                println!("  {}", p.display());
            }
        }
    }

    // B 中独有
    for (md5, b_paths) in &b_map {
        if !a_map.contains_key(md5) {
            println!("{} {}", format!("在 B 独有 (MD5 = {})", md5).blue(), "");
            for p in b_paths {
                println!("  {}", p.display());
                b_unique.push(p.clone());
            }
        }
    }
    
    Ok((b_duplicates, b_unique))
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("{}",
            "用法: chart_compare <源目录> <目标目录> [操作]\n可选操作:\n  [y] 删除重复\n  [o] 输出重复\n  [u] 输出独有".red()
        );
        exit(1);
    }
    let dir_a = Path::new(&args[1]);
    let dir_b = Path::new(&args[2]);
    let input = args.get(3);

    let (b_duplicates, b_unique) = compare_folders(dir_a, dir_b)?;
    println!("{}", format!("共找到 {} 个重复文件", b_duplicates.len()).cyan());
    println!("{}", format!("共找到 {} 个 B 中独有文件", b_unique.len()).cyan());

    let input = if let Some(input) = input {
        input.clone()
    } else {
        println!("{}", format!(
            "比较完成，请选择操作 ({})\n  [y] 删除 B 文件夹中重复文件\n  [o] 输出重复文件列表到 BSame_files.txt\n  [u] 输出 B 独有文件列表到 BUnique_files.txt: ",
            dir_b.display()).yellow()
        );
        let mut user_input = String::new();
        std::io::stdin().read_line(&mut user_input)?;
        let user_input = user_input.trim().to_lowercase();
        user_input
    };

    if input.contains("y") {
        for file in &b_duplicates {
            if fs::remove_file(&file).is_ok() {
                println!("{}", format!("已删除 {}", file.display()).green());
            } else {
                println!("{}", format!("删除失败 {}", file.display()).red());
            }
        }
        println!("{}", "删除任务完成".green());
    }
    if input.contains("o") {
        let mut output_file = File::create("BSame_files.txt")
            .with_context(|| format!("无法创建 BSame_files.txt"))?;
        
        for file in &b_duplicates {
            writeln!(output_file, "{}", file.display())
                .with_context(|| format!("无法写入: {}", file.display()))?;
        }
        println!("{}", format!("重复文件列表已输出到 BSame_files.txt").green());
    }
    if input.contains("u") {
        let mut output_file = File::create("BUnique_files.txt")
            .with_context(|| format!("无法创建 BUnique_files.txt"))?;
        
        for file in &b_unique {
            writeln!(output_file, "{}", file.display())
                .with_context(|| format!("无法写入: {}", file.display()))?;
        }
        println!("{}", format!("B 中独有文件列表已输出到 BUnique_files.txt").green());
    }
    Ok(())
}
