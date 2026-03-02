use std::collections::HashMap;

/// === Модуль для работы с удаленными хранилищами ===

#[derive(Debug, Clone)]
pub struct Remote {
    pub name: String,
    pub r#type: String,
    pub config: HashMap<String, String>,
}

pub mod remotes;

