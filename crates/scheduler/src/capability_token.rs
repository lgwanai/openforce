use std::sync::Arc;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use dashmap::DashMap;
use uuid::Uuid;
use tracing::{info, warn};

use openforce_domain::token::{CapabilityToken, TokenScope};

/// Issues and manages capability tokens on lease grants (architecture doc section 22.3).
pub struct CapabilityTokenIssuer {
    signing_key: Ed25519KeyPair,
    issued: Arc<DashMap<Uuid, CapabilityToken>>,
    revoked: Arc<DashMap<Uuid, String>>,
}

impl CapabilityTokenIssuer {
    pub fn new(pkcs8_der: &[u8]) -> Result<Self, String> {
        let signing_key = Ed25519KeyPair::from_pkcs8(pkcs8_der)
            .map_err(|e| format!("invalid ed25519 key: {e}"))?;
        Ok(Self { signing_key, issued: Arc::new(DashMap::new()), revoked: Arc::new(DashMap::new()) })
    }

    pub fn generate_key() -> Vec<u8> {
        let rng = SystemRandom::new();
        Ed25519KeyPair::generate_pkcs8(&rng).expect("CRNG").as_ref().to_vec()
    }

    pub fn public_key_bytes(&self) -> Vec<u8> {
        self.signing_key.public_key().as_ref().to_vec()
    }

    pub fn issue(
        &self, tenant_id: Uuid, session_id: Uuid, task_id: Uuid,
        task_attempt: i32, lease_id: Uuid, fencing_token: u64,
        plan_epoch: i32, scopes: Vec<TokenScope>,
    ) -> Result<String, String> {
        let jti = Uuid::now_v7();
        let exp = chrono::Utc::now() + chrono::Duration::seconds(300);

        let mut token = CapabilityToken {
            iss: "scheduler.swarmos.internal".into(),
            sub: format!("worker:{}", jti),
            tenant_id, session_id, task_id, task_attempt,
            lease_id, fencing_token, plan_epoch,
            scope: scopes, jti, token_epoch: 1, exp,
            signature: vec![],
        };

        let json = token.canonical_json().to_string();
        token.signature = self.signing_key.sign(json.as_bytes()).as_ref().to_vec();

        self.issued.insert(jti, token.clone());
        info!("token issued: jti={jti} task={task_id} lease={lease_id} fencing={fencing_token}");
        self.encode_token(&token)
    }

    pub fn verify(&self, encoded: &str) -> Result<CapabilityToken, String> {
        let token = self.decode_token(encoded)?;
        if self.revoked.contains_key(&token.jti) { return Err("token revoked".into()); }
        if token.is_expired() { return Err("token expired".into()); }
        self.verify_signature(&token)?;
        Ok(token)
    }

    pub fn revoke(&self, jti: Uuid, reason: &str) {
        self.revoked.insert(jti, reason.into());
        self.issued.remove(&jti);
        warn!("token revoked: jti={jti} reason={reason}");
    }

    pub fn revoke_by_lease(&self, lease_id: Uuid) {
        let jtis: Vec<Uuid> = self.issued.iter()
            .filter_map(|e| if e.lease_id == lease_id { Some(e.jti) } else { None })
            .collect();
        for jti in jtis { self.revoke(jti, "lease cancelled"); }
    }

    fn sign_canonical(&self, token: &CapabilityToken) -> Vec<u8> {
        let json = token.canonical_json().to_string();
        self.signing_key.sign(json.as_bytes()).as_ref().to_vec()
    }

    fn verify_signature(&self, token: &CapabilityToken) -> Result<(), String> {
        let json = token.canonical_json().to_string();
        let pk = ring::signature::UnparsedPublicKey::new(
            &ring::signature::ED25519,
            self.signing_key.public_key().as_ref(),
        );
        pk.verify(json.as_bytes(), &token.signature).map_err(|_| "invalid signature".into())
    }

    fn encode_token(&self, token: &CapabilityToken) -> Result<String, String> {
        let json = serde_json::to_vec(token).map_err(|e| format!("encode: {e}"))?;
        use base64::Engine;
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&json))
    }

    fn decode_token(&self, encoded: &str) -> Result<CapabilityToken, String> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded.as_bytes())
            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(encoded.as_bytes()))
            .map_err(|e| format!("base64: {e}"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("parse: {e}"))
    }

    /// Parse token without cryptographic verification (for extracting claims).
    pub fn decode_only(encoded: &str) -> Result<CapabilityToken, String> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded.as_bytes())
            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(encoded.as_bytes()))
            .map_err(|e| format!("base64: {e}"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("parse: {e}"))
    }
}
