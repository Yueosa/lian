use anyhow::Result;
use chrono::Local;
use std::fs;
use std::path::PathBuf;

pub struct ReportSaver {
    base_dir: PathBuf,
}

impl ReportSaver {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn save(&self, content: &str, distro_name: &str) -> Result<PathBuf> {
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

        // 添加元数据头部（纯文本格式）
        let mut full_content = String::new();
        full_content.push_str(&format!(
            "{} 更新报告\n生成时间: {}\n\n",
            distro_name,
            now.format("%Y-%m-%d %H:%M:%S")
        ));
        full_content.push_str(content);

        // 保存文件
        fs::write(&filepath, full_content)?;

        Ok(filepath)
    }
}
