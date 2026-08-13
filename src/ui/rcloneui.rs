use super::*;
use crate::operations::remotes::ConfigStep;
use crate::operations::{self, CopyOptions, DeleteOptions, MoveOptions};
use crate::rclone_install::RcloneApp;
use eframe::egui;
use egui::{FontData, FontDefinitions, FontFamily};
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::Instant;
use tokio::runtime::Runtime;

impl RcloneUI {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        log::debug!("Инициализация RcloneUI");
        
        // --- ВОССТАНОВЛЕНИЕ ШРИФТОВ ---
        let mut fonts = FontDefinitions::default();

        if let Ok(font_data) = std::fs::read("assets/fonts/Sans.otf") {
            fonts
                .font_data
                .insert("sans".to_owned(), FontData::from_owned(font_data));
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "sans".to_owned());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("sans".to_owned());
            log::debug!("Шрифт Sans.otf загружен");
        } else {
            log::warn!("Файл шрифта Sans.otf не найден");
        }

        if let Ok(emoji_data) = std::fs::read("assets/fonts/NotoColorEmoji.ttf") {
            fonts
                .font_data
                .insert("emoji".to_owned(), FontData::from_owned(emoji_data));
            let prop = fonts.families.entry(FontFamily::Proportional).or_default();
            if !prop.contains(&"emoji".to_owned()) {
                prop.push("emoji".to_owned());
            }
            log::debug!("Шрифт NotoColorEmoji.ttf загружен");
        } else {
            log::warn!("Файл шрифта NotoColorEmoji.ttf не найден");
        }
        cc.egui_ctx.set_fonts(fonts);

        let (init_tx, init_rx) = channel();
        let (op_tx, op_rx) = channel();

        log::info!("Запуск фоновой инициализации rclone");
        std::thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            let result = rt.block_on(async { RcloneApp::new().await });
            let res = match result {
                Ok(app) => {
                    log::info!("Rclone успешно инициализирован");
                    Ok(Arc::new(app))
                }
                Err(e) => {
                    log::error!("Ошибка инициализации rclone: {}", e);
                    Err(e.to_string())
                }
            };
            let _ = init_tx.send(res);
        });

        Self {
            rclone: None,
            state: AppState::Initializing,
            error_message: None,
            current_path: String::new(),
            history_back: Vec::new(),
            history_forward: Vec::new(),
            remote_list: Vec::new(),
            current_files: Vec::new(),
            selected_paths: Vec::new(),
            transfer_source_info: Vec::new(),
            transfer_dest: String::new(),
            active_transfer_tab: TransferTab::Remote,
            show_transfer_dialog: false,
            show_local_browser: false,
            local_browser_path: std::env::var("HOME").unwrap_or_else(|_| "/".into()),
            local_browser_files: Vec::new(),
            is_move_mode: false,
            settings: AppSettings::default(),
            show_add_remote_dialog: false,
            new_remote_name: String::new(),
            new_remote_type: "drive".to_string(),
            add_remote_step: AddRemoteStep::Form,
            add_remote_state: None,
            add_remote_answer: String::new(),
            add_remote_status: String::new(),
            active_task_count: 0,
            operation_tx: op_tx,
            operation_rx: op_rx,
            pending_load_path: None,
            rclone_init_receiver: Some(init_rx),
            active_operations: Vec::new(),
            next_op_id: 1,
        }
    }

    // --- Валидация ---
    pub fn is_path_valid(&self) -> bool {
        if self.transfer_dest.is_empty() {
            return false;
        }
        match self.active_transfer_tab {
            TransferTab::Remote => self.transfer_dest.contains(':'),
            TransferTab::Local => {
                !self.transfer_dest.contains(':') || Path::new(&self.transfer_dest).is_absolute()
            }
        }
    }

    // --- Навигация ---
    pub fn navigate_to(&mut self, new_path: String) {
        log::debug!("Навигация: {} -> {}", self.current_path, new_path);
        if self.current_path == new_path {
            return;
        }
        if !self.current_path.is_empty() {
            self.history_back.push(self.current_path.clone());
        }
        self.history_forward.clear();
        self.pending_load_path = Some(new_path);
    }

    pub fn go_back(&mut self) {
        if let Some(prev) = self.history_back.pop() {
            log::debug!("Назад: {} -> {}", self.current_path, prev);
            self.history_forward.push(self.current_path.clone());
            self.pending_load_path = Some(prev);
        }
    }

    pub fn go_forward(&mut self) {
        if let Some(next) = self.history_forward.pop() {
            log::debug!("Вперед: {} -> {}", self.current_path, next);
            self.history_back.push(self.current_path.clone());
            self.pending_load_path = Some(next);
        }
    }

    pub fn refresh_local_list(&mut self) {
        log::debug!("Обновление списка локальной директории: {}", self.local_browser_path);
        if let Ok(files) = operations::local_fs::list_directory(&self.local_browser_path) {
            let count = files.len();
            self.local_browser_files = files;
            log::debug!("Найдено {} элементов в локальной директории", count);
        } else {
            log::warn!("Не удалось прочитать локальную директорию: {}", self.local_browser_path);
        }
    }

    // --- Работа с Rclone ---
    pub fn poll_rclone_init(&mut self) {
        if let Some(rx) = &self.rclone_init_receiver {
            if let Ok(res) = rx.try_recv() {
                match res {
                    Ok(rclone) => {
                        log::info!("Rclone инициализирован, загрузка удаленных хранилищ");
                        self.rclone = Some(rclone);
                        self.load_remotes();
                        self.state = AppState::Ready;
                    }
                    Err(e) => {
                        log::error!("Ошибка инициализации: {}", e);
                        self.error_message = Some(e.clone());
                        self.state = AppState::Error(e);
                    }
                }
                self.rclone_init_receiver = None;
            }
        }
    }

    /// Фоновый цикл создания remote: rclone задает вопросы (--non-interactive),
    /// на вопрос config_is_local автоматически отвечаем "true" — открывается
    /// браузер для авторизации; остальные вопросы передаются в UI.
    fn run_add_remote_loop(
        rclone: Arc<RcloneApp>,
        name: String,
        rtype: String,
        initial_state: Option<String>,
        initial_answer: Option<String>,
        op_id: u32,
        tx: Sender<OperationResult>,
    ) {
        std::thread::spawn(move || {
            let mut state = initial_state;
            let mut answer = initial_answer;
            loop {
                let feed_tx = tx.clone();
                let step = operations::remotes::config_create_step(
                    &rclone,
                    &name,
                    &rtype,
                    state.as_deref(),
                    answer.as_deref(),
                    &mut |line: &str| {
                        if !line.is_empty() {
                            let _ = feed_tx.send(OperationResult::ProgressUpdate(
                                op_id,
                                0.0,
                                line.to_string(),
                            ));
                        }
                    },
                );
                match step {
                    Ok(ConfigStep::Done) => {
                        log::info!("Remote {} успешно создан", name);
                        let _ = tx.send(match operations::remotes::list(&rclone) {
                            Ok(list) => OperationResult::RemoteAdded(op_id, list),
                            Err(e) => OperationResult::Failure(
                                op_id,
                                format!("Облако создано, но список обновить не удалось: {}", e),
                            ),
                        });
                        return;
                    }
                    Ok(ConfigStep::Question { state: s, question: q }) => {
                        if q.name == "config_is_local" {
                            log::info!("Открытие браузера для авторизации {}", rtype);
                            state = Some(s);
                            answer = Some("true".to_string());
                            let _ = tx.send(OperationResult::ProgressUpdate(
                                op_id,
                                0.0,
                                "Открыт браузер для авторизации. Ожидание входа...".into(),
                            ));
                            continue;
                        }
                        log::debug!("rclone задал вопрос: {}", q.name);
                        let _ = tx.send(OperationResult::ConfigQuestion(op_id, s, q));
                        return;
                    }
                    Err(e) => {
                        log::error!("Ошибка создания remote {}: {}", name, e);
                        let _ = tx.send(OperationResult::Failure(op_id, e));
                        return;
                    }
                }
            }
        });
    }

    pub fn start_add_remote(&mut self) {
        let name = self.new_remote_name.trim().to_string();
        let rtype = self.new_remote_type.trim().to_string();
        if name.is_empty() || rtype.is_empty() {
            log::warn!("Не удалось создать remote: пустое имя или тип");
            return;
        }
        if let Some(rclone) = self.rclone.clone() {
            log::info!("Создание нового remote: {} типа {}", name, rtype);
            self.active_task_count += 1;
            self.add_remote_step = AddRemoteStep::Busy;
            self.add_remote_status = "Создание облака...".into();
            let op_id = self.add_operation(format!("Создание облака {}", name));
            Self::run_add_remote_loop(rclone, name, rtype, None, None, op_id, self.operation_tx.clone());
        }
    }

    pub fn answer_add_remote(&mut self) {
        if let (Some(rclone), Some(state)) = (self.rclone.clone(), self.add_remote_state.clone()) {
            let answer = self.add_remote_answer.clone();
            let name = self.new_remote_name.clone();
            let rtype = self.new_remote_type.clone();
            log::info!("Ответ на вопрос rclone: {}", answer);
            self.active_task_count += 1;
            self.add_remote_step = AddRemoteStep::Busy;
            self.add_remote_status = "Отправка ответа...".into();
            let op_id = self.add_operation(format!("Создание облака {}", name));
            Self::run_add_remote_loop(rclone, name, rtype, Some(state), Some(answer), op_id, self.operation_tx.clone());
        }
    }

    pub fn load_remotes(&mut self) {        if let Some(rclone) = self.rclone.clone() {
            log::info!("Загрузка списка удаленных хранилищ");
            self.active_task_count += 1;
            let tx = self.operation_tx.clone();
            std::thread::spawn(move || {
                log::debug!("Фоновый поток: получение списка remotes");
                let _ = tx.send(match operations::remotes::list(&rclone) {
                    Ok(list) => {
                        log::info!("Загружено {} удаленных хранилищ", list.len());
                        OperationResult::RemoteList(list)
                    }
                    Err(e) => {
                        log::error!("Ошибка загрузки remotes: {}", e);
                        OperationResult::Failure(0, e.to_string())
                    }
                });
            });
        }
    }

    pub fn load_files(&mut self, path: &str) {
        if let Some(rclone) = self.rclone.clone() {
            log::info!("Загрузка файлов из пути: {}", path);
            self.active_task_count += 1;
            let path = path.to_string();
            let tx = self.operation_tx.clone();
            std::thread::spawn(move || {
                log::debug!("Фоновый поток: загрузка файлов из {}", path);
                let _ = tx.send(match operations::files::list(&rclone, &path) {
                    Ok(list) => {
                        log::info!("Загружено {} файлов из {}", list.len(), path);
                        OperationResult::FileList(list)
                    }
                    Err(e) => {
                        log::error!("Ошибка загрузки файлов из {}: {}", path, e);
                        OperationResult::Failure(0, e.to_string())
                    }
                });
            });
        }
    }

    pub fn poll_background_operation(&mut self) {
        while let Ok(result) = self.operation_rx.try_recv() {
            match result {
                OperationResult::Success(op_id, _) => {
                    log::debug!("Операция {} успешно завершена", op_id);
                    self.active_task_count = self.active_task_count.saturating_sub(1);
                    self.active_operations.retain(|op| op.id != op_id);
                    self.pending_load_path = Some(self.current_path.clone());
                }
                OperationResult::Failure(op_id, e) => {
                    log::error!("Операция {} завершена с ошибкой: {}", op_id, e);
                    self.active_task_count = self.active_task_count.saturating_sub(1);
                    self.active_operations.retain(|op| op.id != op_id);
                    if matches!(self.add_remote_step, AddRemoteStep::Busy) {
                        self.add_remote_step = AddRemoteStep::Form;
                    }
                    self.error_message = Some(e);
                }
                OperationResult::FileList(files) => {
                    let count = files.len();
                    log::debug!("Получен список файлов: {} элементов", count);
                    self.current_files = files;
                    self.active_task_count = self.active_task_count.saturating_sub(1);
                }
                OperationResult::RemoteList(remotes) => {
                    let count = remotes.len();
                    log::debug!("Получен список remotes: {} элементов", count);
                    self.remote_list = remotes;
                    self.active_task_count = self.active_task_count.saturating_sub(1);
                }
                OperationResult::RemoteAdded(op_id, remotes) => {
                    let count = remotes.len();
                    log::info!("Облако добавлено, список обновлен: {} элементов", count);
                    self.remote_list = remotes;
                    self.active_task_count = self.active_task_count.saturating_sub(1);
                    self.active_operations.retain(|op| op.id != op_id);
                    self.show_add_remote_dialog = false;
                    self.add_remote_step = AddRemoteStep::Form;
                    self.add_remote_state = None;
                    self.new_remote_name.clear();
                    self.add_remote_answer.clear();
                }
                OperationResult::ConfigQuestion(op_id, state, question) => {
                    log::debug!("Получен вопрос rclone: {}", question.name);
                    self.active_task_count = self.active_task_count.saturating_sub(1);
                    self.active_operations.retain(|op| op.id != op_id);
                    self.add_remote_state = Some(state);
                    self.add_remote_answer = question.default.clone();
                    self.add_remote_step = AddRemoteStep::Question(question);
                }
                OperationResult::ProgressUpdate(op_id, progress, status) => {
                    if let Some(op) = self.active_operations.iter_mut().find(|op| op.id == op_id) {
                        op.progress = progress;
                        op.status = status.clone();
                        log::trace!("Операция {}: прогресс {:.1}%, статус: {}", op_id, progress * 100.0, status);
                    }
                    if matches!(&self.add_remote_step, AddRemoteStep::Busy) {
                        self.add_remote_status = status;
                    }
                }
            }
        }
    }

    pub fn perform_transfer(&mut self, is_move: bool) {
        if self.active_task_count > 0 {
            log::warn!("Не удалось начать трансфер: активны другие операции ({})", self.active_task_count);
            return;
        }
        if let Some(rclone) = self.rclone.clone() {
            let sources = self.transfer_source_info.clone();
            let dest_base = self.transfer_dest.clone();
            let op_id = self.add_operation(format!(
                "{} {} эл.",
                if is_move {
                    "Перенос"
                } else {
                    "Копия"
                },
                sources.len()
            ));
            
            log::info!("Начало операции {}: {} элементов в {}", 
                if is_move { "перемещения" } else { "копирования" },
                sources.len(),
                dest_base
            );
            
            self.active_task_count += 1;
            let tx = self.operation_tx.clone();

            std::thread::spawn(move || {
                let total = sources.len() as f32;
                for (idx, (source, is_dir)) in sources.iter().enumerate() {
                    let name = source.split([':', '/', '\\']).last().unwrap_or("");
                    let separator = if dest_base.contains(':')
                        && !dest_base.ends_with(':')
                        && !dest_base.ends_with('/')
                    {
                        "/"
                    } else {
                        ""
                    };

                    let final_dest = if *is_dir {
                        format!(
                            "{}{}{}",
                            dest_base.trim_end_matches(['/', '\\']),
                            separator,
                            name
                        )
                    } else {
                        dest_base.clone()
                    };

                    log::debug!("Операция {}: обработка {}/{}: {} -> {}", op_id, idx + 1, total, source, final_dest);

                    let res = if is_move {
                        operations::sync::move_files(
                            &rclone,
                            source,
                            &final_dest,
                            &MoveOptions {
                                verbose: true,
                                ..Default::default()
                            },
                        )
                    } else {
                        operations::sync::copy(
                            &rclone,
                            source,
                            &final_dest,
                            &CopyOptions {
                                verbose: true,
                                ..Default::default()
                            },
                        )
                    };

                    if let Err(e) = res {
                        log::error!("Операция {}: ошибка при обработке {}: {}", op_id, name, e);
                        let _ = tx.send(OperationResult::Failure(
                            op_id,
                            format!("Ошибка на {}: {}", name, e),
                        ));
                        return;
                    }
                    
                    let _ = tx.send(OperationResult::ProgressUpdate(
                        op_id,
                        (idx + 1) as f32 / total,
                        format!("{}/{}", idx + 1, total),
                    ));
                }
                log::info!("Операция {} успешно завершена", op_id);
                let _ = tx.send(OperationResult::Success(op_id, "Ок".into()));
            });
        }
    }

    pub fn delete_selected(&mut self) {
        if self.active_task_count > 0 {
            log::warn!("Не удалось начать удаление: активны другие операции ({})", self.active_task_count);
            return;
        }
        if let Some(rclone) = self.rclone.clone() {
            let paths = self.get_selected_info();
            let op_id = self.add_operation(format!("Удаление {} эл.", paths.len()));
            
            log::info!("Начало операции удаления {} элементов", paths.len());
            
            self.active_task_count += 1;
            let tx = self.operation_tx.clone();
            std::thread::spawn(move || {
                for (p, is_dir) in paths.iter() {
                    log::debug!("Операция {}: удаление {}", op_id, p);
                    let opts = DeleteOptions {
                        recursive: *is_dir,
                        ..Default::default()
                    };
                    if let Err(e) = operations::sync::delete(&rclone, p, &opts) {
                        log::error!("Операция {}: ошибка при удалении {}: {}", op_id, p, e);
                        let _ = tx.send(OperationResult::Failure(op_id, e.to_string()));
                        return;
                    }
                }
                log::info!("Операция удаления {} успешно завершена", op_id);
                let _ = tx.send(OperationResult::Success(op_id, "Ок".into()));
            });
        }
    }

    pub fn add_operation(&mut self, description: String) -> u32 {
        let id = self.next_op_id;
        self.next_op_id += 1;
        log::debug!("Добавлена операция {}: {}", id, description);
        self.active_operations.push(Operation {
            id,
            description,
            progress: 0.0,
            status: "В очереди".into(),
            start_time: Instant::now(),
        });
        id
    }

    pub fn get_selected_info(&self) -> Vec<(String, bool)> {
        self.selected_paths
            .iter()
            .map(|path| {
                let is_dir = self
                    .current_files
                    .iter()
                    .find(|f| {
                        let full = if self.current_path.ends_with(':') {
                            format!("{}{}", self.current_path, f.name)
                        } else {
                            format!("{}/{}", self.current_path, f.name)
                        };
                        &full == path
                    })
                    .map(|f| f.is_dir)
                    .unwrap_or(false);
                (path.clone(), is_dir)
            })
            .collect()
    }
}
