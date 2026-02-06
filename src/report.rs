use anyhow::Result;
use chrono::Local;
use std::fs;
use std::path::PathBuf;

pub struct ReportSaver {
    base_dir: PathBuf,
}

#[allow(dead_code)]
impl ReportSaver {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn save(&self, content: &str) -> Result<PathBuf> {
        // 创建基础目录
        fs::create_dir_all(&self.base_dir)?;

        // 获取当前时间
        let now = Local::now();

        // 创建目录结构: YYYY/MM/DD/
        let year = now.format("%Y").to_string();
        let month = now.format("%m").to_string();
        let day = now.format("%d").to_string();

        let dir = self.base_dir.join(&year).join(&month).join(&day);
        fs::create_dir_all(&dir)?;

        // 文件名: HH-mm.md
        let filename = now.format("%H-%M.md").to_string();
        let filepath = dir.join(filename);

        // 添加元数据头部
        let mut full_content = String::new();
        full_content.push_str(&format!("# Arch Linux 更新报告\n\n"));
        full_content.push_str(&format!(
            "**生成时间**: {}\n\n",
            now.format("%Y-%m-%d %H:%M:%S")
        ));
        full_content.push_str("---\n\n");
        full_content.push_str(content);

        // 保存文件
        fs::write(&filepath, full_content)?;

        Ok(filepath)
    }

    pub fn get_latest_reports(&self, count: usize) -> Result<Vec<PathBuf>> {
        let mut reports = Vec::new();

        // 遍历目录获取所有报告文件
        self.collect_reports(&self.base_dir, &mut reports)?;

        // 按修改时间排序
        reports.sort_by(|a, b| {
            let a_meta = fs::metadata(a).ok();
            let b_meta = fs::metadata(b).ok();

            match (a_meta, b_meta) {
                (Some(a_m), Some(b_m)) => {
                    let a_time = a_m.modified().ok();
                    let b_time = b_m.modified().ok();
                    b_time.cmp(&a_time)
                }
                _ => std::cmp::Ordering::Equal,
            }
        });

        // 取前 N 个
        reports.truncate(count);

        Ok(reports)
    }

    fn collect_reports(&self, dir: &PathBuf, reports: &mut Vec<PathBuf>) -> Result<()> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    self.collect_reports(&path, reports)?;
                } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
                    reports.push(path);
                }
            }
        }
        Ok(())
    }
}
