//! PackageManager 相关数据类型定义

/// 命令输出结果
#[derive(Debug, Clone)]
pub struct UpdateOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

impl UpdateOutput {
    pub fn combined_output(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

/// 搜索结果条目
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub repo: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub installed: bool,
}

/// 包详情
#[derive(Debug, Clone)]
pub struct PackageDetail {
    pub fields: Vec<(String, String)>,
}

/// 已安装包信息
#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    pub size: String,
    pub description: String,
}
