from pathlib import Path


def rep(path, old, new):
    p = Path(path)
    s = p.read_text(encoding='utf-8')
    if old not in s:
        raise SystemExit(f'missing pattern in {path}: {old[:100]!r}')
    p.write_text(s.replace(old, new, 1), encoding='utf-8')


desktop = 'crates/hermes-desktop/src/lib.rs'
rep(desktop, '    sync::{Arc, Mutex, RwLock},', '    sync::{Arc, Mutex, OnceLock, RwLock},')
rep(desktop, 'use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};', 'use base64::{Engine as _, engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD}};')
rep(desktop, '    AgentConfigSnapshot, AppSettings, AuthProvider, AuxiliaryModels, ConfigSchemaResponse,\n', '    AgentConfigSnapshot, AppSettings, AttachmentKind, AuthProvider, AuxiliaryModels, ConfigSchemaResponse,\n')
rep(desktop, '    RuntimeStatus, SessionCreateRequest, SessionCreateResponse, SessionDirectiveResult,\n', '    RuntimeStatus, SelectedAttachment, SessionAttachmentResult, SessionCreateRequest, SessionCreateResponse, SessionDirectiveResult,\n')
rep(desktop, '''#[derive(Clone)]
struct GatewayServices {
    client: Arc<RwLock<Option<GatewayClient>>>,
    rest: Arc<RwLock<Option<GatewayRest>>>,
    connection_store: Arc<ConnectionConfigStore>,
}
''', '''#[derive(Clone)]
struct GatewayServices {
    client: Arc<RwLock<Option<GatewayClient>>>,
    rest: Arc<RwLock<Option<GatewayRest>>>,
    connection_store: Arc<ConnectionConfigStore>,
}

const MAX_ATTACHMENT_SELECTIONS: usize = 256;
const MAX_IMAGE_PREVIEW_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Default)]
struct AttachmentSelectionStore {
    paths: Mutex<HashMap<String, PathBuf>>,
}

static ATTACHMENT_SELECTIONS: OnceLock<AttachmentSelectionStore> = OnceLock::new();

fn attachment_selections() -> &'static AttachmentSelectionStore {
    ATTACHMENT_SELECTIONS.get_or_init(AttachmentSelectionStore::default)
}

impl AttachmentSelectionStore {
    fn register(&self, path: &Path) -> ServiceResult<String> {
        let mut paths = self.paths.lock().map_err(|_| {
            ServiceError::Platform("attachment selection store lock was poisoned".into())
        })?;
        if paths.len() >= MAX_ATTACHMENT_SELECTIONS {
            paths.clear();
        }
        let id = Uuid::new_v4().to_string();
        paths.insert(id.clone(), path.to_owned());
        Ok(id)
    }

    fn resolve(&self, id: &str) -> ServiceResult<PathBuf> {
        if id.is_empty() || id.len() > 128 {
            return Err(ServiceError::InvalidInput("invalid attachment selection".into()));
        }
        self.paths
            .lock()
            .map_err(|_| ServiceError::Platform("attachment selection store lock was poisoned".into()))?
            .get(id)
            .cloned()
            .ok_or_else(|| ServiceError::NotFound("attachment selection expired".into()))
    }
}

fn attachment_image_mime(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "tif" | "tiff" => Some("image/tiff"),
        _ => None,
    }
}

fn selected_attachment(path: &Path) -> ServiceResult<SelectedAttachment> {
    let metadata = fs::metadata(path).map_err(platform)?;
    if !metadata.is_file() {
        return Err(ServiceError::InvalidInput(format!(
            "attachment is not a file: {}",
            path.display()
        )));
    }
    let kind = if attachment_image_mime(path).is_some() {
        AttachmentKind::Image
    } else {
        AttachmentKind::File
    };
    let preview_data_url = if kind == AttachmentKind::Image && metadata.len() <= MAX_IMAGE_PREVIEW_BYTES {
        let mime = attachment_image_mime(path).unwrap_or("application/octet-stream");
        let bytes = fs::read(path).map_err(platform)?;
        Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
    } else {
        None
    };
    Ok(SelectedAttachment {
        id: attachment_selections().register(path)?,
        kind,
        label: path.file_name().and_then(|value| value.to_str()).unwrap_or("attachment").to_owned(),
        size: metadata.len(),
        preview_data_url,
        ..SelectedAttachment::default()
    })
}
''')
rep(desktop, '''impl PlatformService for DesktopPlatform {
    fn pick_folder(''', '''impl PlatformService for DesktopPlatform {
    fn pick_attachments(
        &self,
        title: &str,
        starting_directory: Option<&Path>,
        images_only: bool,
    ) -> ServiceFuture<'_, Vec<SelectedAttachment>> {
        let title = title.to_owned();
        let starting_directory = starting_directory.map(Path::to_owned);
        Box::pin(async move {
            let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
            if let Some(directory) = starting_directory.filter(|path| path.is_dir()) {
                dialog = dialog.set_directory(directory);
            }
            if images_only {
                dialog = dialog.add_filter(
                    "Images",
                    &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff"],
                );
            }
            let handles = dialog.pick_files().await.unwrap_or_default();
            handles
                .into_iter()
                .map(|handle| selected_attachment(handle.path()))
                .collect()
        })
    }

    fn pick_folder(''')

print('desktop attachment selection transform applied')
