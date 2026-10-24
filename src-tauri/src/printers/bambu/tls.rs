//! Manufacturer trust anchors, TLS 1.2, and device identity instead of DHCP hostname.
//!
//! Bambu's legacy device certificates are X.509 v1. webpki rejects that version,
//! so this verifier checks a deliberately small chain itself with the rustls provider.
use std::{fmt, sync::Arc};

use rumqttc::tokio_rustls::rustls::{
    self,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::{self, WebPkiSupportedAlgorithms},
    pki_types::{alg_id, pem::PemObject, CertificateDer, ServerName, UnixTime},
    CertificateError, ClientConfig, DigitallySignedStruct, Error, SignatureScheme,
};
use x509_parser::{extensions::GeneralName, prelude::*, x509::AlgorithmIdentifier};

const BAMBU_CA: &[u8] = include_bytes!("../../../certs/bambu.cert");

#[derive(Debug, thiserror::Error)]
#[error("BambuTlsConfiguration")]
pub struct ConfigError;

fn roots_from_pem(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>, ConfigError> {
    let mut roots = Vec::new();
    for cert in CertificateDer::pem_slice_iter(pem) {
        let cert = cert.map_err(|_| ConfigError)?;
        parse_cert(&cert).map_err(|_| ConfigError)?;
        roots.push(cert);
    }
    if roots.is_empty() {
        return Err(ConfigError);
    }
    Ok(roots)
}

pub fn client_config(serial: &str) -> Result<ClientConfig, ConfigError> {
    let provider = Arc::new(crypto::aws_lc_rs::default_provider());
    let verifier = DeviceVerifier {
        roots: roots_from_pem(BAMBU_CA)?,
        serial: serial.to_owned(),
        algorithms: provider.signature_verification_algorithms,
    };
    Ok(ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS12])
        .map_err(|_| ConfigError)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth())
}

struct DeviceVerifier {
    roots: Vec<CertificateDer<'static>>,
    serial: String,
    algorithms: WebPkiSupportedAlgorithms,
}

fn invalid(reason: CertificateError) -> Error {
    Error::InvalidCertificate(reason)
}

fn parse_cert<'a>(der: &'a CertificateDer<'_>) -> Result<X509Certificate<'a>, Error> {
    let (rest, cert) = X509Certificate::from_der(der.as_ref())
        .map_err(|_| invalid(CertificateError::BadEncoding))?;
    if !rest.is_empty() {
        return Err(invalid(CertificateError::BadEncoding));
    }
    Ok(cert)
}

fn validate_time(cert: &X509Certificate<'_>, now: UnixTime) -> Result<(), Error> {
    let now = i64::try_from(now.as_secs()).map_err(|_| invalid(CertificateError::Expired))?;
    if now < cert.validity().not_before.timestamp() {
        return Err(invalid(CertificateError::NotValidYet));
    }
    if now > cert.validity().not_after.timestamp() {
        return Err(invalid(CertificateError::Expired));
    }
    Ok(())
}

fn validate_ca(cert: &X509Certificate<'_>, now: UnixTime) -> Result<(), Error> {
    validate_time(cert, now)?;
    if cert.version() == X509Version::V3 {
        let constraints = cert
            .basic_constraints()
            .map_err(|_| invalid(CertificateError::BadEncoding))?
            .ok_or_else(|| invalid(CertificateError::BadEncoding))?;
        if !constraints.value.ca {
            return Err(invalid(CertificateError::BadEncoding));
        }
    }
    if let Some(usage) = cert
        .key_usage()
        .map_err(|_| invalid(CertificateError::BadEncoding))?
    {
        if !usage.value.key_cert_sign() {
            return Err(invalid(CertificateError::BadEncoding));
        }
    }
    Ok(())
}

/// Compare the entire DER AlgorithmIdentifier, including parameters and EC curve.
fn algorithm_matches(
    x509: &AlgorithmIdentifier<'_>,
    provider: alg_id::AlgorithmIdentifier,
) -> bool {
    // rustls-pki-types stores the SEQUENCE *content*, while x509-parser
    // expects its complete DER encoding.
    if provider.as_ref().len() > 127 {
        return false;
    }
    let mut der = Vec::with_capacity(provider.as_ref().len() + 2);
    der.extend_from_slice(&[0x30, provider.as_ref().len() as u8]);
    der.extend_from_slice(provider.as_ref());
    matches!(AlgorithmIdentifier::from_der(&der), Ok(([], parsed)) if parsed == *x509)
}

fn verify_cert_signature(
    cert: &X509Certificate<'_>,
    signer: &X509Certificate<'_>,
    algorithms: &WebPkiSupportedAlgorithms,
) -> Result<(), Error> {
    if cert.tbs_certificate.signature != cert.signature_algorithm
        || cert.signature_value.unused_bits != 0
        || signer.public_key().subject_public_key.unused_bits != 0
    {
        return Err(invalid(CertificateError::BadEncoding));
    }
    let public_key = signer.public_key();
    for algorithm in algorithms.all {
        if algorithm_matches(&public_key.algorithm, algorithm.public_key_alg_id())
            && algorithm_matches(&cert.signature_algorithm, algorithm.signature_alg_id())
            && algorithm
                .verify_signature(
                    public_key.subject_public_key.data.as_ref(),
                    cert.tbs_certificate.as_ref(),
                    cert.signature_value.data.as_ref(),
                )
                .is_ok()
        {
            return Ok(());
        }
    }
    Err(invalid(CertificateError::BadSignature))
}

impl fmt::Debug for DeviceVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BambuDeviceVerifier")
    }
}

/// Only absent device names permit skipping the identity check. Present but
/// mismatched names, malformed certificates, or malformed SANs always fail closed.
fn verify_device_name(cert: &CertificateDer<'_>, serial: &str) -> Result<(), Error> {
    let bad_encoding = || Error::InvalidCertificate(CertificateError::BadEncoding);
    let (rest, parsed) = X509Certificate::from_der(cert.as_ref()).map_err(|_| bad_encoding())?;
    if !rest.is_empty() {
        return Err(bad_encoding());
    }
    let mut found = false;
    let mut matched = false;
    for cn in parsed.subject().iter_common_name() {
        let name = cn.as_str().map_err(|_| bad_encoding())?;
        if !name.is_empty() {
            found = true;
            matched |= name == serial;
        }
    }
    if let Some(san) = parsed
        .subject_alternative_name()
        .map_err(|_| bad_encoding())?
    {
        for name in &san.value.general_names {
            // IP addresses are transport locations, not a printer's device identity.
            if let GeneralName::DNSName(name)
            | GeneralName::URI(name)
            | GeneralName::RFC822Name(name) = name
            {
                if !name.is_empty() {
                    found = true;
                    matched |= *name == serial;
                }
            }
        }
    }
    if found && !matched {
        return Err(Error::InvalidCertificate(CertificateError::NotValidForName));
    }
    Ok(())
}

impl DeviceVerifier {
    fn verify_anchor(&self, anchor: &X509Certificate<'_>, now: UnixTime) -> Result<(), Error> {
        validate_ca(anchor, now)?;
        if anchor.issuer().as_raw() == anchor.subject().as_raw() {
            return verify_cert_signature(anchor, anchor, &self.algorithms);
        }
        // Some bundled CA2 certificates are signed by the bundled BBL CA.
        // The parent itself must be a self-signed, trusted CA.
        for parent_der in &self.roots {
            let parent = parse_cert(parent_der)?;
            if anchor.issuer().as_raw() == parent.subject().as_raw()
                && parent.issuer().as_raw() == parent.subject().as_raw()
                && validate_ca(&parent, now).is_ok()
                && verify_cert_signature(&parent, &parent, &self.algorithms).is_ok()
                && verify_cert_signature(anchor, &parent, &self.algorithms).is_ok()
            {
                return Ok(());
            }
        }
        Err(invalid(CertificateError::UnknownIssuer))
    }

    fn verify_chain(
        &self,
        leaf: &X509Certificate<'_>,
        intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<(), Error> {
        if intermediates.len() > 1 {
            return Err(invalid(CertificateError::UnknownIssuer));
        }
        validate_time(leaf, now)?;
        let intermediate = match intermediates.first() {
            Some(der) => {
                let cert = parse_cert(der)?;
                validate_ca(&cert, now)?;
                if leaf.issuer().as_raw() != cert.subject().as_raw() {
                    return Err(invalid(CertificateError::UnknownIssuer));
                }
                Some(cert)
            }
            None => None,
        };
        let child = intermediate.as_ref().unwrap_or(leaf);
        for anchor_der in &self.roots {
            let anchor = parse_cert(anchor_der)?;
            if child.issuer().as_raw() == anchor.subject().as_raw()
                && self.verify_anchor(&anchor, now).is_ok()
                && verify_cert_signature(child, &anchor, &self.algorithms).is_ok()
                && (intermediate.is_none()
                    || verify_cert_signature(leaf, child, &self.algorithms).is_ok())
            {
                return Ok(());
            }
        }
        Err(invalid(CertificateError::UnknownIssuer))
    }

    fn verify_tls12_message(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        scheme: SignatureScheme,
        signature: &[u8],
    ) -> Result<HandshakeSignatureValid, Error> {
        let leaf = parse_cert(cert)?;
        if leaf.public_key().subject_public_key.unused_bits != 0 {
            return Err(invalid(CertificateError::BadEncoding));
        }
        let (_, candidates) = self
            .algorithms
            .mapping
            .iter()
            .find(|(candidate, _)| *candidate == scheme)
            .ok_or_else(|| invalid(CertificateError::BadSignature))?;
        for algorithm in *candidates {
            if algorithm_matches(&leaf.public_key().algorithm, algorithm.public_key_alg_id())
                && algorithm
                    .verify_signature(
                        leaf.public_key().subject_public_key.data.as_ref(),
                        message,
                        signature,
                    )
                    .is_ok()
            {
                return Ok(HandshakeSignatureValid::assertion());
            }
        }
        Err(invalid(CertificateError::BadSignature))
    }
}

impl ServerCertVerifier for DeviceVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let cert = parse_cert(end_entity)?;
        self.verify_chain(&cert, intermediates, now)?;
        verify_device_name(end_entity, &self.serial)?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.verify_tls12_message(message, cert, dss.scheme, dss.signature())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Err(Error::General("TLS 1.3 disabled".into()))
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::internal::msgs::codec::{Codec, Reader};
    use std::time::Duration;

    const SERIAL: &str = "00M00A000000000";
    const TEST_ROOT: &[u8] = include_bytes!("fixtures/tls/root.pem");
    const LEAF: &[u8] = include_bytes!("fixtures/tls/device.pem");
    const CN_ONLY: &[u8] = include_bytes!("fixtures/tls/cn_only.pem");
    const NO_NAME: &[u8] = include_bytes!("fixtures/tls/no_name.pem");
    const V1_ROOT: &[u8] = include_bytes!("fixtures/tls/v1_root.pem");
    const V1_LEAF: &[u8] = include_bytes!("fixtures/tls/v1_device.pem");
    const V1_INTERMEDIATE: &[u8] = include_bytes!("fixtures/tls/v1_intermediate.pem");
    const V1_CHAINED: &[u8] = include_bytes!("fixtures/tls/v1_chained.pem");
    const V1_TLS12_SIGNATURE: &[u8] = include_bytes!("fixtures/tls/v1_tls12.sig");
    const TLS12_MESSAGE: &[u8] = b"fake tls12 handshake message";

    fn cert(pem: &[u8]) -> CertificateDer<'_> {
        CertificateDer::from_pem_slice(pem).unwrap()
    }

    fn verifier(roots: Vec<CertificateDer<'static>>, serial: &str) -> DeviceVerifier {
        DeviceVerifier {
            roots,
            serial: serial.into(),
            algorithms: crypto::aws_lc_rs::default_provider().signature_verification_algorithms,
        }
    }

    fn check(
        verifier: &DeviceVerifier,
        leaf: &[u8],
        time: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        verifier.verify_server_cert(
            &cert(leaf),
            &[],
            &ServerName::try_from("192.168.1.50").unwrap(),
            &[],
            time,
        )
    }

    fn check_with_intermediate(
        verifier: &DeviceVerifier,
        leaf: &[u8],
        intermediates: &[CertificateDer<'_>],
        time: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        verifier.verify_server_cert(
            &cert(leaf),
            intermediates,
            &ServerName::try_from("192.168.1.50").unwrap(),
            &[],
            time,
        )
    }

    // Fixed time within the synthetic fixture validity, independent of the test runner's clock.
    fn valid_time() -> UnixTime {
        let leaf = cert(LEAF);
        let (_, parsed) = X509Certificate::from_der(leaf.as_ref()).unwrap();
        UnixTime::since_unix_epoch(Duration::from_secs(
            parsed.validity().not_before.timestamp() as u64 + 60,
        ))
    }

    fn v1_valid_time() -> UnixTime {
        let leaf = cert(V1_LEAF);
        let (_, parsed) = X509Certificate::from_der(leaf.as_ref()).unwrap();
        assert_eq!(parsed.version(), X509Version::V1);
        UnixTime::since_unix_epoch(Duration::from_secs(
            parsed.validity().not_before.timestamp() as u64 + 60,
        ))
    }

    fn signed_struct(scheme: SignatureScheme, signature: &[u8]) -> DigitallySignedStruct {
        let mut encoded = Vec::new();
        scheme.encode(&mut encoded);
        encoded.extend_from_slice(&u16::try_from(signature.len()).unwrap().to_be_bytes());
        encoded.extend_from_slice(signature);
        DigitallySignedStruct::read(&mut Reader::init(&encoded)).unwrap()
    }

    #[test]
    fn bambu_tls_loads_embedded_ca_only() {
        let roots = roots_from_pem(BAMBU_CA).unwrap();
        assert_eq!(
            roots.len(),
            CertificateDer::pem_slice_iter(BAMBU_CA).count()
        );
        assert!(!roots.is_empty());
        assert!(client_config(SERIAL).is_ok());
        assert!(roots_from_pem(b"invalid").is_err());
        assert!(check(&verifier(roots, SERIAL), LEAF, valid_time()).is_err());
    }

    #[test]
    fn bambu_tls_embedded_ca_signatures_are_valid() {
        let roots = roots_from_pem(BAMBU_CA).unwrap();
        let verifier = verifier(roots.clone(), SERIAL);
        // A fixed date lies within every bundled CA's validity interval.
        let now = UnixTime::since_unix_epoch(Duration::from_secs(1_767_225_600));
        for der in &roots {
            assert!(verifier
                .verify_anchor(&parse_cert(der).unwrap(), now)
                .is_ok());
        }
    }

    #[test]
    fn bambu_tls_valid_chain_and_serial_san_ignore_dhcp_ip() {
        let verifier = verifier(roots_from_pem(TEST_ROOT).unwrap(), SERIAL);
        assert!(check(&verifier, LEAF, valid_time()).is_ok());
        assert!(check(&verifier, CN_ONLY, valid_time()).is_ok());
    }

    #[test]
    fn bambu_tls_rejects_serial_mismatch() {
        let verifier = verifier(roots_from_pem(TEST_ROOT).unwrap(), "00M00A000000001");
        assert!(matches!(
            check(&verifier, LEAF, valid_time()),
            Err(Error::InvalidCertificate(CertificateError::NotValidForName))
        ));
        assert!(check(&verifier, CN_ONLY, valid_time()).is_err());
    }

    #[test]
    fn bambu_tls_missing_name_still_requires_trusted_chain() {
        assert!(check(
            &verifier(roots_from_pem(TEST_ROOT).unwrap(), SERIAL),
            NO_NAME,
            valid_time()
        )
        .is_ok());
        assert!(check(
            &verifier(roots_from_pem(BAMBU_CA).unwrap(), SERIAL),
            NO_NAME,
            valid_time()
        )
        .is_err());
    }

    #[test]
    fn bambu_tls_rejects_expired_and_not_yet_valid_certificates() {
        let verifier = verifier(roots_from_pem(TEST_ROOT).unwrap(), SERIAL);
        assert!(check(&verifier, LEAF, UnixTime::since_unix_epoch(Duration::ZERO)).is_err());
        assert!(check(
            &verifier,
            LEAF,
            UnixTime::since_unix_epoch(Duration::from_secs(4_000_000_000))
        )
        .is_err());
    }

    #[test]
    fn bambu_tls_rejects_corrupt_signature_and_malformed_identity() {
        let verifier = verifier(roots_from_pem(TEST_ROOT).unwrap(), SERIAL);
        let mut der = cert(LEAF).as_ref().to_vec();
        *der.last_mut().unwrap() ^= 1;
        assert!(verifier
            .verify_server_cert(
                &CertificateDer::from(der),
                &[],
                &ServerName::try_from("192.168.1.50").unwrap(),
                &[],
                valid_time()
            )
            .is_err());
        assert!(verify_device_name(&CertificateDer::from(vec![0, 1]), SERIAL).is_err());
        assert_eq!(format!("{verifier:?}"), "BambuDeviceVerifier");
    }

    #[test]
    fn bambu_tls_accepts_v1_leaf_signed_by_v3_ca() {
        let verifier = verifier(roots_from_pem(V1_ROOT).unwrap(), SERIAL);
        assert!(check(&verifier, V1_LEAF, v1_valid_time()).is_ok());
    }

    #[test]
    fn bambu_tls_v1_rejects_wrong_root() {
        let verifier = verifier(roots_from_pem(TEST_ROOT).unwrap(), SERIAL);
        assert!(check(&verifier, V1_LEAF, v1_valid_time()).is_err());
    }

    #[test]
    fn bambu_tls_v1_rejects_expired_and_not_yet_valid() {
        let verifier = verifier(roots_from_pem(V1_ROOT).unwrap(), SERIAL);
        assert!(check(
            &verifier,
            V1_LEAF,
            UnixTime::since_unix_epoch(Duration::ZERO)
        )
        .is_err());
        assert!(check(
            &verifier,
            V1_LEAF,
            UnixTime::since_unix_epoch(Duration::from_secs(4_000_000_000))
        )
        .is_err());
    }

    #[test]
    fn bambu_tls_v1_rejects_serial_mismatch() {
        let verifier = verifier(roots_from_pem(V1_ROOT).unwrap(), "00M00A000000001");
        assert!(matches!(
            check(&verifier, V1_LEAF, v1_valid_time()),
            Err(Error::InvalidCertificate(CertificateError::NotValidForName))
        ));
    }

    #[test]
    fn bambu_tls_v1_rejects_tampered_signature() {
        let verifier = verifier(roots_from_pem(V1_ROOT).unwrap(), SERIAL);
        let mut der = cert(V1_LEAF).as_ref().to_vec();
        *der.last_mut().unwrap() ^= 1;
        assert!(verifier
            .verify_server_cert(
                &CertificateDer::from(der),
                &[],
                &ServerName::try_from("192.168.1.50").unwrap(),
                &[],
                v1_valid_time()
            )
            .is_err());
    }

    #[test]
    fn bambu_tls12_verifies_v1_leaf_handshake_signature() {
        let verifier = verifier(roots_from_pem(V1_ROOT).unwrap(), SERIAL);
        let dss = signed_struct(SignatureScheme::RSA_PKCS1_SHA256, V1_TLS12_SIGNATURE);
        assert!(verifier
            .verify_tls12_signature(TLS12_MESSAGE, &cert(V1_LEAF), &dss)
            .is_ok());
        assert!(verifier
            .verify_tls12_signature(b"tampered message", &cert(V1_LEAF), &dss)
            .is_err());
        let bad_scheme = signed_struct(SignatureScheme::ECDSA_NISTP256_SHA256, V1_TLS12_SIGNATURE);
        assert!(verifier
            .verify_tls12_signature(TLS12_MESSAGE, &cert(V1_LEAF), &bad_scheme)
            .is_err());
        assert!(verifier
            .verify_tls13_signature(TLS12_MESSAGE, &cert(V1_LEAF), &dss)
            .is_err());
    }

    #[test]
    fn bambu_tls_v1_single_intermediate_chain_is_bounded() {
        let verifier = verifier(roots_from_pem(V1_ROOT).unwrap(), SERIAL);
        let intermediate = cert(V1_INTERMEDIATE);
        assert!(check_with_intermediate(
            &verifier,
            V1_CHAINED,
            std::slice::from_ref(&intermediate),
            v1_valid_time()
        )
        .is_ok());
        assert!(check(&verifier, V1_CHAINED, v1_valid_time()).is_err());
        assert!(check_with_intermediate(
            &verifier,
            V1_CHAINED,
            &[intermediate.clone(), intermediate],
            v1_valid_time()
        )
        .is_err());
    }
}
