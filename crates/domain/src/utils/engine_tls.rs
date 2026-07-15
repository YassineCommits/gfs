//! Apply provider-specific TLS server settings onto a [`ComputeDefinition`].

use std::path::Path;

use crate::ports::compute::ComputeDefinition;

/// Container-relative paths used by all engine TLS applicators.
pub const SERVER_CERT: &str = "server.crt";
pub const SERVER_KEY: &str = "server.key";

/// Append SSL/TLS listen configuration for the given database provider.
///
/// Cert files are expected under `container_tls_dir` as `server.crt` / `server.key`.
pub fn apply_engine_tls(definition: &mut ComputeDefinition, provider: &str, container_tls_dir: &Path) {
    let cert = container_tls_dir.join(SERVER_CERT);
    let key = container_tls_dir.join(SERVER_KEY);
    let cert_s = cert.to_string_lossy();
    let key_s = key.to_string_lossy();

    match provider.to_ascii_lowercase().as_str() {
        "postgres" | "postgresql" => {
            definition.args.extend([
                "-c".into(),
                "ssl=on".into(),
                "-c".into(),
                format!("ssl_cert_file={cert_s}"),
                "-c".into(),
                format!("ssl_key_file={key_s}"),
            ]);
        }
        "mysql" => {
            // Official mysql image reads --ssl-* flags.
            definition.args.extend([
                format!("--ssl-cert={cert_s}"),
                format!("--ssl-key={key_s}"),
            ]);
        }
        "clickhouse" => {
            // Mount only for now; full HTTPS listener config is provider/image specific.
            // Leaf + CA are still present for future CH config.d snippets.
            tracing::debug!(
                "clickhouse TLS material mounted at {}; listener SSL config not auto-applied",
                container_tls_dir.display()
            );
        }
        other => {
            tracing::debug!(provider = %other, "no TLS applicator args for provider");
        }
    }
}
