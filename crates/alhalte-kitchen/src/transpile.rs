//! Transpilation engine converting serialized session logs into standalone HTML5 or EPUB 3.0 documents.

use std::io::{Cursor, Write};

use alhalte_saignant::escape::{escape_attribute, escape_html};
use alhalte_saignant::{full_css, render_log_entry};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::error::Result;
use crate::serialize::SerializedSession;

/// Options for configuring EPUB 3.0 generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpubOptions {
    /// Publication title.
    pub title: String,
    /// Primary author or creator name.
    pub author: String,
    /// Unique identifier for the publication (URN/UUID).
    pub identifier: String,
    /// BCP 47 language code (e.g. `"ja"`, `"en"`).
    pub language: String,
    /// W3CDTF modification timestamp (e.g. `"2026-10-17T00:00:00Z"`).
    pub modified: String,
}

impl Default for EpubOptions {
    fn default() -> Self {
        Self {
            title: "Alhalte Session".to_string(),
            author: "Alhalte Kitchen".to_string(),
            identifier: format!("urn:uuid:{}", Uuid::new_v4()),
            language: "ja".to_string(),
            modified: "2026-10-17T00:00:00Z".to_string(),
        }
    }
}

/// Transpiles a [`SerializedSession`] into a complete, standalone HTML5 document.
pub fn transpile_to_html5(session: &SerializedSession) -> String {
    let mut out = String::with_capacity(32768);

    out.push_str("<!DOCTYPE html>\n");
    out.push_str("<html lang=\"ja\">\n");
    out.push_str("<head>\n");
    out.push_str("  <meta charset=\"utf-8\" />\n");
    out.push_str(
        "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n",
    );
    out.push_str("  <title>");
    out.push_str(&escape_html(&session.title));
    out.push_str("</title>\n");
    out.push_str("  <style>\n");
    out.push_str(&full_css());
    out.push_str("\n/* Kitchen HTML5 Layout */\n");
    out.push_str(":root {\n  --kitchen-max-width: 960px;\n}\n");
    out.push_str("body {\n");
    out.push_str("  margin: 0;\n");
    out.push_str("  padding: 1.5rem;\n");
    out.push_str("  background-color: var(--md-sys-color-background, #fffbfe);\n");
    out.push_str("  color: var(--md-sys-color-on-background, #1c1b1f);\n");
    out.push_str("  font-family: system-ui, -apple-system, sans-serif;\n");
    out.push_str("  line-height: 1.5;\n");
    out.push_str("}\n");
    out.push_str(
        ".kitchen-container {\n  max-width: var(--kitchen-max-width);\n  margin: 0 auto;\n}\n",
    );
    out.push_str(".kitchen-session-header {\n  margin-bottom: 2rem;\n  padding-bottom: 1rem;\n  border-bottom: 2px solid var(--md-sys-color-outline, #79747e);\n}\n");
    out.push_str(".kitchen-scene {\n  margin-bottom: 2.5rem;\n}\n");
    out.push_str(".kitchen-scene-title {\n  margin-top: 1.5rem;\n  margin-bottom: 1rem;\n  padding-bottom: 0.5rem;\n  border-bottom: 1px solid var(--md-sys-color-outline-variant, #cac4d0);\n  font-size: 1.5rem;\n}\n");
    out.push_str(
        ".kitchen-scene-logs {\n  display: flex;\n  flex-direction: column;\n  gap: 0.75rem;\n}\n",
    );
    out.push_str("  </style>\n");
    out.push_str("</head>\n");
    out.push_str("<body>\n");
    out.push_str("  <div class=\"kitchen-container\">\n");
    out.push_str("    <header class=\"kitchen-session-header\">\n");
    out.push_str("      <h1>");
    out.push_str(&escape_html(&session.title));
    out.push_str("</h1>\n");
    out.push_str("    </header>\n");
    out.push_str("    <main>\n");

    for scene in &session.scenes {
        out.push_str("      <section class=\"kitchen-scene\" id=\"");
        out.push_str(&escape_attribute(&scene.scene_id));
        out.push_str("\">\n");
        out.push_str("        <h2 class=\"kitchen-scene-title\">");
        out.push_str(&escape_html(&scene.title));
        out.push_str("</h2>\n");
        out.push_str("        <div class=\"kitchen-scene-logs\">\n");

        for log in &scene.logs {
            out.push_str("          ");
            out.push_str(&render_log_entry(log, &session.character_names));
            out.push('\n');
        }

        out.push_str("        </div>\n");
        out.push_str("      </section>\n");
    }

    out.push_str("    </main>\n");
    out.push_str("  </div>\n");
    out.push_str("</body>\n");
    out.push_str("</html>\n");

    out
}

/// Transpiler producing EPUB 3.0 compliant binaries.
pub struct EpubTranspiler;

impl EpubTranspiler {
    /// Transpiles a [`SerializedSession`] into an EPUB 3.0 binary.
    pub fn transpile(session: &SerializedSession, options: &EpubOptions) -> Result<Vec<u8>> {
        let buf = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(buf);

        // 1. mimetype (MUST be first entry, uncompressed / stored, exactly 20 bytes)
        let stored_opt = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("mimetype", stored_opt)?;
        zip.write_all(b"application/epub+zip")?;

        let deflate_opt =
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        // 2. META-INF/container.xml
        zip.start_file("META-INF/container.xml", deflate_opt)?;
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
        )?;

        // Ensure at least one chapter exists
        let chapters: Vec<(String, String)> = if session.scenes.is_empty() {
            vec![("Empty Session".to_string(), String::new())]
        } else {
            session
                .scenes
                .iter()
                .map(|s| {
                    let mut logs_html = String::new();
                    for log in &s.logs {
                        logs_html.push_str(&render_log_entry(log, &session.character_names));
                        logs_html.push('\n');
                    }
                    (s.title.clone(), logs_html)
                })
                .collect()
        };

        // 3. OEBPS/content.opf
        zip.start_file("OEBPS/content.opf", deflate_opt)?;
        let mut opf = String::new();
        opf.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        opf.push_str("<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"pub-id\">\n");
        opf.push_str("  <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n");
        opf.push_str(&format!(
            "    <dc:identifier id=\"pub-id\">{}</dc:identifier>\n",
            escape_html(&options.identifier)
        ));
        opf.push_str(&format!(
            "    <dc:title>{}</dc:title>\n",
            escape_html(&options.title)
        ));
        opf.push_str(&format!(
            "    <dc:creator>{}</dc:creator>\n",
            escape_html(&options.author)
        ));
        opf.push_str(&format!(
            "    <dc:language>{}</dc:language>\n",
            escape_html(&options.language)
        ));
        opf.push_str(&format!(
            "    <meta property=\"dcterms:modified\">{}</meta>\n",
            escape_html(&options.modified)
        ));
        opf.push_str("  </metadata>\n");
        opf.push_str("  <manifest>\n");
        opf.push_str("    <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n");
        opf.push_str("    <item id=\"style\" href=\"style.css\" media-type=\"text/css\"/>\n");

        for i in 1..=chapters.len() {
            opf.push_str(&format!(
                "    <item id=\"chapter_{i}\" href=\"chapter_{i}.xhtml\" media-type=\"application/xhtml+xml\"/>\n"
            ));
        }

        opf.push_str("  </manifest>\n");
        opf.push_str("  <spine>\n");

        for i in 1..=chapters.len() {
            opf.push_str(&format!("    <itemref idref=\"chapter_{i}\"/>\n"));
        }

        opf.push_str("  </spine>\n");
        opf.push_str("</package>\n");
        zip.write_all(opf.as_bytes())?;

        // 4. OEBPS/nav.xhtml
        zip.start_file("OEBPS/nav.xhtml", deflate_opt)?;
        let mut nav = String::new();
        nav.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        nav.push_str("<!DOCTYPE html>\n");
        nav.push_str(&format!(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" lang=\"{}\" xml:lang=\"{}\">\n",
            escape_attribute(&options.language),
            escape_attribute(&options.language)
        ));
        nav.push_str("  <head>\n");
        nav.push_str("    <meta charset=\"utf-8\" />\n");
        nav.push_str("    <title>Table of Contents</title>\n");
        nav.push_str("    <link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\" />\n");
        nav.push_str("  </head>\n");
        nav.push_str("  <body>\n");
        nav.push_str("    <nav epub:type=\"toc\" id=\"toc\">\n");
        nav.push_str("      <h1>Table of Contents</h1>\n");
        nav.push_str("      <ol>\n");

        for (i, (title, _)) in chapters.iter().enumerate() {
            nav.push_str(&format!(
                "        <li><a href=\"chapter_{}.xhtml\">{}</a></li>\n",
                i + 1,
                escape_html(title)
            ));
        }

        nav.push_str("      </ol>\n");
        nav.push_str("    </nav>\n");
        nav.push_str("  </body>\n");
        nav.push_str("</html>\n");
        zip.write_all(nav.as_bytes())?;

        // 5. OEBPS/style.css
        zip.start_file("OEBPS/style.css", deflate_opt)?;
        zip.write_all(full_css().as_bytes())?;

        // 6. Chapters: OEBPS/chapter_*.xhtml
        for (i, (title, logs_html)) in chapters.iter().enumerate() {
            let filename = format!("OEBPS/chapter_{}.xhtml", i + 1);
            zip.start_file(filename, deflate_opt)?;

            let mut chapter_xml = String::new();
            chapter_xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
            chapter_xml.push_str("<!DOCTYPE html>\n");
            chapter_xml.push_str(&format!(
                "<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" lang=\"{}\" xml:lang=\"{}\">\n",
                escape_attribute(&options.language),
                escape_attribute(&options.language)
            ));
            chapter_xml.push_str("  <head>\n");
            chapter_xml.push_str("    <meta charset=\"utf-8\" />\n");
            chapter_xml.push_str(&format!("    <title>{}</title>\n", escape_html(title)));
            chapter_xml
                .push_str("    <link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\" />\n");
            chapter_xml.push_str("  </head>\n");
            chapter_xml.push_str("  <body>\n");
            chapter_xml.push_str("    <section epub:type=\"chapter\" role=\"doc-chapter\">\n");
            chapter_xml.push_str(&format!("      <h1>{}</h1>\n", escape_html(title)));
            chapter_xml.push_str("      <div class=\"kitchen-scene-logs\">\n");
            chapter_xml.push_str(logs_html);
            chapter_xml.push_str("      </div>\n");
            chapter_xml.push_str("    </section>\n");
            chapter_xml.push_str("  </body>\n");
            chapter_xml.push_str("</html>\n");

            zip.write_all(chapter_xml.as_bytes())?;
        }

        let cursor = zip.finish()?;
        Ok(cursor.into_inner())
    }
}

/// Convenience function to transpile a [`SerializedSession`] into an EPUB binary with optional title.
pub fn transpile_to_epub(session: &SerializedSession, title: Option<&str>) -> Result<Vec<u8>> {
    let mut options = EpubOptions::default();
    if let Some(t) = title {
        options.title = t.to_string();
    } else if !session.title.is_empty() {
        options.title = session.title.clone();
    }
    EpubTranspiler::transpile(session, &options)
}
