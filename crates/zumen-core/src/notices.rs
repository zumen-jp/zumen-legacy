use crate::types::{Notice, NoticeLevel};

/// 省略・代替した内容を種類（code）ごとに数える。最初に現れた順に並べる
#[derive(Debug, Clone, Default)]
pub struct NoticeList {
    items: Vec<Notice>,
}

impl NoticeList {
    pub fn new() -> Self {
        Self::default()
    }

    /// 1 件として数える
    pub fn add(&mut self, level: NoticeLevel, code: &str, message: impl Into<String>) {
        self.add_count(level, code, message, 1);
    }

    pub fn add_count(&mut self, level: NoticeLevel, code: &str, message: impl Into<String>, count: u64) {
        if let Some(existing) = self.items.iter_mut().find(|notice| notice.code == code) {
            existing.count += count;
        } else {
            self.items.push(Notice { level, code: code.to_string(), message: message.into(), count });
        }
    }

    pub fn into_vec(self) -> Vec<Notice> {
        self.items
    }
}
