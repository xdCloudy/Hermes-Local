use std::{fs, io::Read, path::Path};

use hermes_core::{
    AppServices, PreviewDocument, PreviewDocumentKind, PreviewService, ServiceError, ServiceFuture,
};

use crate::preview_normalization::{
    PreviewKind, PreviewNormalizationService, PreviewTarget, TEXT_PREVIEW_MAX_BYTES,
};

#[derive(Clone, Copy, Debug, Default)]
struct DesktopPreviewService;

impl PreviewService for DesktopPreviewService {
    fn load(
        &self,
        raw_target: &str,
        base_dir: Option<&Path>,
    ) -> ServiceFuture<'_, Option<PreviewDocument>> {
        let raw_target = raw_target.to_owned();
        let base_dir = base_dir.map(Path::to_path_buf);
        Box::pin(async move {
            let target = PreviewNormalizationService
                .normalize(&raw_target, base_dir.as_deref())
                .map_err(ServiceError::InvalidInput)?;
            target.map(load_document).transpose()
        })
    }
}

fn load_document(target: PreviewTarget) -> Result<PreviewDocument, ServiceError> {
    match target {
        PreviewTarget::Url { label, source, url } => Ok(PreviewDocument {
            kind: PreviewDocumentKind::Url,
            label,
            source,
            url,
            ..PreviewDocument::default()
        }),
        PreviewTarget::File {
            byte_size,
            large,
            label,
            language,
            mime_type,
            path,
            preview_kind,
            source,
            url,
            ..
        } => {
            let kind = match preview_kind {
                PreviewKind::Html => PreviewDocumentKind::Html,
                PreviewKind::Image => PreviewDocumentKind::Image,
                PreviewKind::Binary => PreviewDocumentKind::Binary,
                PreviewKind::Text => PreviewDocumentKind::Text,
            };
            let text = if !large
                && matches!(kind, PreviewDocumentKind::Html | PreviewDocumentKind::Text)
            {
                Some(read_bounded_text(&path)?)
            } else {
                None
            };
            Ok(PreviewDocument {
                kind,
                label,
                source,
                url,
                mime_type: Some(mime_type),
                language: Some(language),
                byte_size: Some(byte_size),
                large,
                text,
            })
        }
    }
}

fn read_bounded_text(path: &Path) -> Result<String, ServiceError> {
    let mut file = fs::File::open(path).map_err(|error| {
        ServiceError::Platform(format!("Preview target is not readable: {error}"))
    })?;
    let mut bytes = Vec::with_capacity(TEXT_PREVIEW_MAX_BYTES as usize + 1);
    file.take(TEXT_PREVIEW_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            ServiceError::Platform(format!("Could not read preview target: {error}"))
        })?;
    if bytes.len() as u64 > TEXT_PREVIEW_MAX_BYTES {
        return Err(ServiceError::InvalidInput(
            "Preview target grew beyond the 512 KiB inline limit while it was being read.".into(),
        ));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn install(services: &mut AppServices) {
    services.preview = std::sync::Arc::new(DesktopPreviewService);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[tokio::test]
    async fn loads_text_through_the_normalized_native_boundary() {
        let root = test_directory("text");
        fs::write(root.join("hello.txt"), "hello preview\n").expect("seed text");
        let document = DesktopPreviewService
            .load("hello.txt", Some(&root))
            .await
            .expect("load preview")
            .expect("preview document");
        assert_eq!(document.kind, PreviewDocumentKind::Text);
        assert_eq!(document.text.as_deref(), Some("hello preview\n"));
        assert_eq!(document.byte_size, Some(14));
        cleanup(root);
    }

    #[tokio::test]
    async fn large_text_is_classified_without_inlining_contents() {
        let root = test_directory("large");
        fs::write(
            root.join("large.txt"),
            vec![b'x'; TEXT_PREVIEW_MAX_BYTES as usize + 1],
        )
        .expect("seed large file");
        let document = DesktopPreviewService
            .load("large.txt", Some(&root))
            .await
            .expect("load preview")
            .expect("preview document");
        assert_eq!(document.kind, PreviewDocumentKind::Text);
        assert!(document.large);
        assert!(document.text.is_none());
        cleanup(root);
    }

    #[tokio::test]
    async fn binary_content_never_crosses_into_the_ui_dto() {
        let root = test_directory("binary");
        fs::write(root.join("sample.bin"), [0_u8, 1, 2, 3]).expect("seed binary");
        let document = DesktopPreviewService
            .load("sample.bin", Some(&root))
            .await
            .expect("load preview")
            .expect("preview document");
        assert_eq!(document.kind, PreviewDocumentKind::Binary);
        assert!(document.text.is_none());
        cleanup(root);
    }

    fn test_directory(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "hermes-preview-service-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("test directory");
        directory
    }

    fn cleanup(path: PathBuf) {
        let _ = fs::remove_dir_all(path);
    }
}
