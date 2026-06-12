//! 添付ファイルの種類判定などの純粋ロジック。

/// ファイルの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// 画像ファイル（.png, .jpg, .jpeg, .gif, .webp）
    Image,
    /// HEIC/HEIF ファイル（変換が必要）
    Heic,
    /// その他のファイル
    Other,
}

/// ファイル名からファイル種類を判定する。
pub fn classify_file(filename: &str) -> FileType {
    let lower = filename.to_lowercase();

    let image_extensions = [".png", ".jpg", ".jpeg", ".gif", ".webp"];
    if image_extensions.iter().any(|ext| lower.ends_with(ext)) {
        return FileType::Image;
    }

    let heic_extensions = [".heic", ".heif"];
    if heic_extensions.iter().any(|ext| lower.ends_with(ext)) {
        return FileType::Heic;
    }

    FileType::Other
}

/// ファイル名の拡張子から Content-Type を推定する。
pub fn guess_content_type(filename: &str) -> Option<String> {
    mime_guess::from_path(filename)
        .first()
        .map(|mime| mime.to_string())
}

/// スポイラー指定された添付ファイルかどうかを判定する。
pub fn is_spoiler_attachment(filename: &str) -> bool {
    filename.starts_with("SPOILER_")
}

/// スポイラー画像のトグル見出しテキストを生成する。
pub fn spoiler_summary(description: Option<&str>) -> String {
    let mut summary = "Spoiler image".to_string();
    if let Some(description) = description
        .map(str::trim)
        .filter(|description| !description.is_empty())
    {
        summary.push_str("\nALT: ");
        summary.push_str(description);
    }
    summary
}

/// ファイル名の拡張子を置き換える。
pub fn replace_extension(filename: &str, new_ext: &str) -> String {
    if let Some(pos) = filename.rfind('.') {
        format!("{}.{}", &filename[..pos], new_ext)
    } else {
        format!("{}.{}", filename, new_ext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 画像拡張子（png/jpg/jpeg/gif/webp）を持つファイル名が、
    /// 大文字小文字を問わず FileType::Image に分類されることを確認する。
    #[test]
    fn test_classify_file_image() {
        assert_eq!(classify_file("photo.png"), FileType::Image);
        assert_eq!(classify_file("photo.PNG"), FileType::Image);
        assert_eq!(classify_file("image.jpg"), FileType::Image);
        assert_eq!(classify_file("image.JPG"), FileType::Image);
        assert_eq!(classify_file("picture.jpeg"), FileType::Image);
        assert_eq!(classify_file("animation.gif"), FileType::Image);
        assert_eq!(classify_file("modern.webp"), FileType::Image);
    }

    /// heic/heif 拡張子を持つファイル名が、大文字小文字を問わず
    /// FileType::Heic に分類されることを確認する。
    #[test]
    fn test_classify_file_heic() {
        assert_eq!(classify_file("photo.heic"), FileType::Heic);
        assert_eq!(classify_file("photo.HEIC"), FileType::Heic);
        assert_eq!(classify_file("image.heif"), FileType::Heic);
        assert_eq!(classify_file("image.HEIF"), FileType::Heic);
    }

    /// 画像/HEIC 以外の拡張子や拡張子なしのファイル名が、
    /// FileType::Other に分類されることを確認する。
    #[test]
    fn test_classify_file_other() {
        assert_eq!(classify_file("document.pdf"), FileType::Other);
        assert_eq!(classify_file("archive.zip"), FileType::Other);
        assert_eq!(classify_file("script.js"), FileType::Other);
        assert_eq!(classify_file("noextension"), FileType::Other);
    }

    /// ドット無しで拡張子文字列のみで終わる名前（somepng 等）が、
    /// 画像や HEIC と誤判定されず FileType::Other になることを確認する。
    #[test]
    fn test_classify_file_rejects_similar_names() {
        // ドットなしの拡張子文字列で終わるファイル名は画像として判定されない
        assert_eq!(classify_file("somepng"), FileType::Other);
        assert_eq!(classify_file("filejpg"), FileType::Other);
        assert_eq!(classify_file("imageheic"), FileType::Other);
    }

    /// 各種拡張子から推定される Content-Type が期待どおりになり、
    /// 拡張子なしの場合は None になることを確認する。
    #[test]
    fn test_guess_content_type() {
        assert_eq!(
            guess_content_type("photo.heic"),
            Some("image/heic".to_string())
        );
        assert_eq!(
            guess_content_type("photo.HEIC"),
            Some("image/heic".to_string())
        );
        assert_eq!(
            guess_content_type("image.heif"),
            Some("image/heif".to_string())
        );
        assert_eq!(
            guess_content_type("photo.png"),
            Some("image/png".to_string())
        );
        assert_eq!(
            guess_content_type("photo.jpg"),
            Some("image/jpeg".to_string())
        );
        assert_eq!(
            guess_content_type("doc.pdf"),
            Some("application/pdf".to_string())
        );
        assert_eq!(
            guess_content_type("archive.zip"),
            Some("application/zip".to_string())
        );
        assert_eq!(
            guess_content_type("data.gpx"),
            Some("application/gpx+xml".to_string())
        );
        assert_eq!(guess_content_type("noextension"), None);
    }

    /// ファイル名が "SPOILER_" で始まる場合のみスポイラー添付と判定し、
    /// 接頭辞の大文字小文字も区別することを確認する。
    #[test]
    fn test_is_spoiler_attachment() {
        assert!(is_spoiler_attachment("SPOILER_photo.png"));
        assert!(!is_spoiler_attachment("photo.png"));
        assert!(!is_spoiler_attachment("spoiler_photo.png"));
    }

    /// 説明が None または空白のみの場合、ALT 行を付けず
    /// "Spoiler image" のみの見出しが生成されることを確認する。
    #[test]
    fn test_spoiler_summary_without_alt() {
        assert_eq!(spoiler_summary(None), "Spoiler image");
        assert_eq!(spoiler_summary(Some("   ")), "Spoiler image");
    }

    /// 説明が指定された場合、"Spoiler image" に続けて
    /// "ALT: <説明>" 行が付いた見出しが生成されることを確認する。
    #[test]
    fn test_spoiler_summary_with_alt() {
        assert_eq!(
            spoiler_summary(Some("sensitive content")),
            "Spoiler image\nALT: sensitive content"
        );
    }

    /// 既存の拡張子を新しい拡張子へ置き換え、複数ドットを含む名前は
    /// 最後のドット以降のみ置換、拡張子なしの場合は付加されることを確認する。
    #[test]
    fn test_replace_extension() {
        assert_eq!(replace_extension("photo.heic", "jpg"), "photo.jpg");
        assert_eq!(replace_extension("image.HEIC", "jpg"), "image.jpg");
        assert_eq!(replace_extension("my.photo.heic", "jpg"), "my.photo.jpg");
        assert_eq!(replace_extension("noextension", "jpg"), "noextension.jpg");
    }
}
