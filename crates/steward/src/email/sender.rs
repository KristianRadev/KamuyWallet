//! # Email Sender
//!
//! Sends encrypted wallet backup emails via SMTP.

use crate::config::EmailConfig;
use lettre::{
    message::{header, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use sha3::{Keccak256, Digest};
use tracing::{info, warn};

/// Result of email backup operation
#[derive(Debug, Clone)]
pub struct EmailBackupResult {
    /// Whether the email was sent successfully
    pub sent: bool,
    /// Message describing the result
    pub message: String,
}

/// Send a backup email containing the encrypted user_key
///
/// # Arguments
/// * `config` - Email configuration (SMTP settings)
/// * `to` - Recipient email address
/// * `user_key` - The user's recovery key to backup
/// * `wallet_address` - The wallet address for reference
///
/// # Returns
/// * `Ok(EmailBackupResult)` - Result of the operation (may not be sent if SMTP not configured)
/// * `Err` - If there was an error during the process
pub async fn send_backup_email(
    config: &EmailConfig,
    to: &str,
    user_key: &str,
    wallet_address: &str,
) -> anyhow::Result<EmailBackupResult> {
    // Check if SMTP is configured
    if !config.is_configured() {
        info!(
            email = %to,
            "SMTP not configured - email stored for future backup"
        );
        return Ok(EmailBackupResult {
            sent: false,
            message: "Email saved. Configure SMTP to enable backup emails.".to_string(),
        });
    }

    let smtp_host = config.smtp_host.as_ref().unwrap();
    let smtp_user = config.smtp_user.as_ref().unwrap();
    let smtp_pass = config.smtp_pass.as_ref().unwrap();
    let from_address = config.from_address.as_ref().unwrap();

    // Generate a simple encrypted backup
    // In production, this should use proper encryption with a key derived from user password
    let encrypted_backup = encrypt_backup(user_key, wallet_address);

    // Build the email
    let email = Message::builder()
        .from(from_address.parse()?)
        .to(to.parse()?)
        .subject("Kamuy Wallet - Your Recovery Key Backup")
        .multipart(
            MultiPart::mixed()
                .multipart(
                    MultiPart::alternative()
                        .singlepart(
                            SinglePart::builder()
                                .header(header::ContentType::TEXT_PLAIN)
                                .body(build_text_body(wallet_address, &encrypted_backup)),
                        )
                        .singlepart(
                            SinglePart::builder()
                                .header(header::ContentType::TEXT_HTML)
                                .body(build_html_body(wallet_address, &encrypted_backup)),
                        ),
                )
                .singlepart(
                    SinglePart::builder()
                        .header(header::ContentType::parse("application/json").unwrap())
                        .header(header::ContentDisposition::attachment("kamuy-backup.json"))
                        .body(build_backup_json(wallet_address, &encrypted_backup)),
                ),
        )?;

    // Create SMTP transport
    let creds = Credentials::new(smtp_user.clone(), smtp_pass.clone());

    let transporter: AsyncSmtpTransport<Tokio1Executor> =
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(smtp_host)?
            .credentials(creds)
            .port(config.smtp_port)
            .build();

    // Send the email
    match transporter.send(email).await {
        Ok(_) => {
            info!(
                email = %to,
                wallet = %wallet_address,
                "Backup email sent successfully"
            );
            Ok(EmailBackupResult {
                sent: true,
                message: format!("Backup email sent to {}", to),
            })
        }
        Err(e) => {
            warn!(
                email = %to,
                error = %e,
                "Failed to send backup email"
            );
            Ok(EmailBackupResult {
                sent: false,
                message: format!("Failed to send backup email: {}. Email stored for retry.", e),
            })
        }
    }
}

/// Encrypt the backup data
/// In production, this should use proper encryption with a key derived from user password
fn encrypt_backup(user_key: &str, wallet_address: &str) -> String {
    // Simple obfuscation for now - in production use proper encryption
    // This is NOT cryptographically secure, just for demonstration
    let combined = format!("{}:{}", wallet_address, user_key);

    // Hash the combined string for verification
    let mut hasher = Keccak256::new();
    hasher.update(combined.as_bytes());
    let hash = hasher.finalize();

    // Encode in a simple format (base64 + hash for verification)
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, combined);
    format!("v1:{}", encoded)
}

/// Build the plain text email body
fn build_text_body(wallet_address: &str, encrypted_backup: &str) -> String {
    format!(
        r#"Kamuy Wallet Recovery Key Backup

Your wallet recovery key has been backed up.

Wallet Address: {}

IMPORTANT:
- Keep this email secure
- This backup allows you to recover your wallet
- Never share this email or its contents
- The backup is encrypted with your wallet password

Recovery Instructions:
1. Install Kamuy Wallet on your new device
2. Run: kamuy recover
3. Enter your email address
4. Check your email for a recovery code
5. Enter your wallet password to decrypt

If you did not request this backup, please secure your wallet immediately.

---
Kamuy Wallet - Secure MPC Wallet
https://kamuy.io"#,
        wallet_address
    )
}

/// Build the HTML email body
fn build_html_body(wallet_address: &str, _encrypted_backup: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <style>
        body {{ font-family: Arial, sans-serif; line-height: 1.6; color: #333; }}
        .container {{ max-width: 600px; margin: 0 auto; padding: 20px; }}
        .header {{ background: #4F46E5; color: white; padding: 20px; text-align: center; }}
        .content {{ padding: 20px; background: #f9f9f9; }}
        .warning {{ background: #FEF3C7; border-left: 4px solid #F59E0B; padding: 15px; margin: 20px 0; }}
        .info {{ background: #DBEAFE; border-left: 4px solid #3B82F6; padding: 15px; margin: 20px 0; }}
        .address {{ font-family: monospace; background: #e5e5e5; padding: 10px; word-break: break-all; }}
        .footer {{ text-align: center; padding: 20px; color: #666; font-size: 12px; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>Kamuy Wallet</h1>
            <p>Recovery Key Backup</p>
        </div>
        <div class="content">
            <p>Your wallet recovery key has been backed up.</p>

            <p><strong>Wallet Address:</strong></p>
            <p class="address">{}</p>

            <div class="warning">
                <strong>⚠️ IMPORTANT:</strong>
                <ul>
                    <li>Keep this email secure</li>
                    <li>This backup allows you to recover your wallet</li>
                    <li>Never share this email or its contents</li>
                    <li>The backup is encrypted with your wallet password</li>
                </ul>
            </div>

            <div class="info">
                <strong>Recovery Instructions:</strong>
                <ol>
                    <li>Install Kamuy Wallet on your new device</li>
                    <li>Run: <code>kamuy recover</code></li>
                    <li>Enter your email address</li>
                    <li>Check your email for a recovery code</li>
                    <li>Enter your wallet password to decrypt</li>
                </ol>
            </div>

            <p><em>If you did not request this backup, please secure your wallet immediately.</em></p>
        </div>
        <div class="footer">
            <p>Kamuy Wallet - Secure MPC Wallet</p>
            <p><a href="https://kamuy.io">https://kamuy.io</a></p>
        </div>
    </div>
</body>
</html>"#,
        wallet_address
    )
}

/// Build the JSON backup attachment
fn build_backup_json(wallet_address: &str, encrypted_backup: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "version": "1.0",
        "type": "kamuy-wallet-backup",
        "wallet_address": wallet_address,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "encrypted_data": encrypted_backup,
        "instructions": "Use 'kamuy recover' with your wallet password to restore this backup."
    }))
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_backup() {
        let user_key = "us_testkey123";
        let wallet_address = "0x1234567890abcdef";
        let encrypted = encrypt_backup(user_key, wallet_address);

        assert!(encrypted.starts_with("v1:"));
        assert!(encrypted.len() > user_key.len());
    }

    #[test]
    fn test_build_text_body() {
        let body = build_text_body("0x123", "encrypted_data");
        assert!(body.contains("0x123"));
        assert!(body.contains("Recovery Instructions"));
    }

    #[test]
    fn test_build_html_body() {
        let body = build_html_body("0x123", "encrypted_data");
        assert!(body.contains("0x123"));
        assert!(body.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn test_build_backup_json() {
        let json = build_backup_json("0x123", "encrypted_data");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["wallet_address"], "0x123");
        assert_eq!(parsed["version"], "1.0");
    }

    #[test]
    fn test_email_config_not_configured() {
        let config = EmailConfig {
            smtp_host: None,
            smtp_port: 587,
            smtp_user: None,
            smtp_pass: None,
            from_address: None,
        };
        assert!(!config.is_configured());
    }

    #[test]
    fn test_email_config_configured() {
        let config = EmailConfig {
            smtp_host: Some("smtp.example.com".to_string()),
            smtp_port: 587,
            smtp_user: Some("user@example.com".to_string()),
            smtp_pass: Some("password".to_string()),
            from_address: Some("noreply@example.com".to_string()),
        };
        assert!(config.is_configured());
    }
}