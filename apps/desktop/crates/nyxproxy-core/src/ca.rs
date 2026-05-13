//! NyxProxy certificate authority.
//!
//! On first launch the proxy generates a self-signed root CA. The CA private
//! key is written to the user's data directory with restrictive permissions
//! (`0600` on Unix). From then on, every intercepted host gets a leaf
//! certificate minted on-the-fly and signed by this CA.
//!
//! Users must install the root CA in their trust store (or their browser) for
//! HTTPS interception to work without warnings — exactly like Burp's
//! `cacert.der`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose,
    IsCa, KeyPair, KeyUsagePurpose, SanType, SerialNumber,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use time::{Duration, OffsetDateTime};

use crate::error::{NyxError, NyxResult};

const CA_COMMON_NAME: &str = "NyxProxy Root CA";
const CA_ORGANISATION: &str = "NyxProxy";
const CA_VALID_DAYS: i64 = 365 * 10;
const LEAF_VALID_DAYS: i64 = 365 * 2;

/// A persisted root CA used to sign leaf certificates.
#[derive(Clone)]
pub struct CertAuthority {
    inner: Arc<CertAuthorityInner>,
    leaf_cache: Arc<RwLock<HashMap<String, LeafEntry>>>,
}

struct CertAuthorityInner {
    cert_pem: String,
    key_pem: String,
    data_dir: PathBuf,
}

#[derive(Clone)]
struct LeafEntry {
    cert_chain: Vec<CertificateDer<'static>>,
    private_key: Arc<PrivateKeyDer<'static>>,
}

impl CertAuthority {
    /// Load an existing CA from disk, or generate and persist a new one.
    pub fn load_or_generate(data_dir: impl AsRef<Path>) -> NyxResult<Self> {
        let dir = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;
        let cert_path = dir.join("nyxproxy-ca.pem");
        let key_path = dir.join("nyxproxy-ca.key");

        let (cert_pem, key_pem) = if cert_path.exists() && key_path.exists() {
            (
                std::fs::read_to_string(&cert_path)?,
                std::fs::read_to_string(&key_path)?,
            )
        } else {
            let (cert_pem, key_pem) = generate_ca_pem()?;
            std::fs::write(&cert_path, &cert_pem)?;
            std::fs::write(&key_path, &key_pem)?;
            set_private_permissions(&key_path)?;
            (cert_pem, key_pem)
        };

        Ok(Self {
            inner: Arc::new(CertAuthorityInner {
                cert_pem,
                key_pem,
                data_dir: dir,
            }),
            leaf_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// In-memory CA used in tests.
    #[doc(hidden)]
    pub fn ephemeral() -> NyxResult<Self> {
        let (cert_pem, key_pem) = generate_ca_pem()?;
        Ok(Self {
            inner: Arc::new(CertAuthorityInner {
                cert_pem,
                key_pem,
                data_dir: PathBuf::from("/tmp"),
            }),
            leaf_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn cert_pem(&self) -> &str {
        &self.inner.cert_pem
    }

    pub fn key_pem(&self) -> &str {
        &self.inner.key_pem
    }

    pub fn data_dir(&self) -> &Path {
        &self.inner.data_dir
    }

    pub fn ca_cert_path(&self) -> PathBuf {
        self.inner.data_dir.join("nyxproxy-ca.pem")
    }

    /// Get (or mint) a leaf certificate for the given host, ready for use with
    /// a `rustls::ServerConfig`.
    pub fn leaf_for(
        &self,
        host: &str,
    ) -> NyxResult<(Vec<CertificateDer<'static>>, Arc<PrivateKeyDer<'static>>)> {
        let host_lower = host.to_ascii_lowercase();
        if let Some(entry) = self.leaf_cache.read().get(&host_lower) {
            return Ok((entry.cert_chain.clone(), entry.private_key.clone()));
        }

        let (chain, key) = self.mint_leaf(&host_lower)?;
        let key_arc = Arc::new(key);
        self.leaf_cache.write().insert(
            host_lower,
            LeafEntry {
                cert_chain: chain.clone(),
                private_key: key_arc.clone(),
            },
        );
        Ok((chain, key_arc))
    }

    fn mint_leaf(
        &self,
        host: &str,
    ) -> NyxResult<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
        // Re-derive the CA on every mint so the rcgen handles stay scoped here.
        let ca_pair = KeyPair::from_pem(&self.inner.key_pem)?;
        let ca_params = ca_params()?;
        let ca_cert = ca_params.self_signed(&ca_pair)?;

        let mut leaf_params = CertificateParams::default();
        leaf_params.is_ca = IsCa::NoCa;
        let now = OffsetDateTime::now_utc();
        leaf_params.not_before = now - Duration::days(1);
        leaf_params.not_after = now + Duration::days(LEAF_VALID_DAYS);
        leaf_params.serial_number = Some(SerialNumber::from(rand::random::<u64>()));
        leaf_params.subject_alt_names = vec![san_for(host)?];

        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, host);
        dn.push(DnType::OrganizationName, "NyxProxy");
        leaf_params.distinguished_name = dn;
        leaf_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

        let leaf_pair = KeyPair::generate()?;
        let leaf_cert = leaf_params.signed_by(&leaf_pair, &ca_cert, &ca_pair)?;

        let leaf_pem = leaf_cert.pem();
        let ca_pem = self.inner.cert_pem.clone();

        let mut chain: Vec<CertificateDer<'static>> = Vec::with_capacity(2);
        chain.extend(pem_to_certs(&leaf_pem)?);
        chain.extend(pem_to_certs(&ca_pem)?);

        let key_der = leaf_pair.serialize_der();
        let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der));

        Ok((chain, private_key))
    }
}

fn ca_params() -> NyxResult<CertificateParams> {
    let mut params = CertificateParams::default();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, CA_COMMON_NAME);
    dn.push(DnType::OrganizationName, CA_ORGANISATION);
    params.distinguished_name = dn;
    let now = OffsetDateTime::now_utc();
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(CA_VALID_DAYS);
    params.serial_number = Some(SerialNumber::from(1u64));
    Ok(params)
}

fn generate_ca_pem() -> NyxResult<(String, String)> {
    let params = ca_params()?;
    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;
    Ok((cert.pem(), key_pair.serialize_pem()))
}

fn san_for(host: &str) -> NyxResult<SanType> {
    if let Ok(addr) = host.parse::<std::net::IpAddr>() {
        Ok(SanType::IpAddress(addr))
    } else {
        let dns = host
            .try_into()
            .map_err(|_| NyxError::Ca(format!("invalid DNS name: {host}")))?;
        Ok(SanType::DnsName(dns))
    }
}

fn pem_to_certs(pem: &str) -> NyxResult<Vec<CertificateDer<'static>>> {
    let mut out = Vec::new();
    let mut bytes: &[u8] = pem.as_bytes();
    while let Some((item, rest)) = rustls_pemfile::read_one_from_slice(bytes)
        .map_err(|e| NyxError::Ca(format!("pem parse: {e:?}")))?
    {
        bytes = rest;
        if let rustls_pemfile::Item::X509Certificate(c) = item {
            out.push(c.into_owned());
        }
    }
    if out.is_empty() {
        return Err(NyxError::Ca("no certificates found in PEM".into()));
    }
    Ok(out)
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> NyxResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> NyxResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_a_root_ca() {
        let ca = CertAuthority::ephemeral().expect("ca generated");
        assert!(ca.cert_pem().contains("BEGIN CERTIFICATE"));
        assert!(ca.key_pem().contains("PRIVATE KEY"));
    }

    #[test]
    fn mints_a_leaf_for_a_host() {
        let ca = CertAuthority::ephemeral().expect("ca generated");
        let (chain, _key) = ca.leaf_for("example.com").expect("leaf minted");
        assert_eq!(chain.len(), 2, "chain should include leaf + CA");
    }

    #[test]
    fn leaves_are_cached_per_host() {
        let ca = CertAuthority::ephemeral().expect("ca generated");
        let (c1, _) = ca.leaf_for("example.com").unwrap();
        let (c2, _) = ca.leaf_for("example.com").unwrap();
        assert_eq!(c1[0].as_ref(), c2[0].as_ref());
    }

    #[test]
    fn persists_to_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let _ca = CertAuthority::load_or_generate(tmp.path()).unwrap();
        assert!(tmp.path().join("nyxproxy-ca.pem").exists());
        assert!(tmp.path().join("nyxproxy-ca.key").exists());
    }

    #[test]
    fn handles_ip_addresses() {
        let ca = CertAuthority::ephemeral().expect("ca generated");
        let (chain, _) = ca.leaf_for("127.0.0.1").expect("leaf minted");
        assert!(!chain.is_empty());
    }
}
