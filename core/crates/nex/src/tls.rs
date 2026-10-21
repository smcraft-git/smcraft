//! TLS for `wss://` game servers, with the server's key pinned.
//!
//! Nextendo's game servers present a self-signed certificate (their own, for the host names
//! the Switch asks for), so a CA check cannot work; instead the certificate's public key must
//! be the expected one: the SHA-256 of its SubjectPublicKeyInfo equals the pin. Anything else
//! fails the handshake before a byte of NEX is sent; there is no "accept anything" fallback.
//! The handshake's own signature is still checked against that key, and ALPN offers only
//! http/1.1 (the WebSocket upgrade).

use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, ring, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, ClientConfig, DigitallySignedStruct, SignatureScheme};

use crate::Error;
use crate::client::Connector;
use crate::crypto::sha256;
use crate::ws::Transport;

/// The SubjectPublicKeyInfo of a DER certificate (the whole element, tag and length included).
pub fn spki(cert: &[u8]) -> Option<&[u8]> {
    // (tag, header length, content length) of the element at the start of `b`
    fn tlv(b: &[u8]) -> Option<(u8, usize, usize)> {
        let tag = *b.first()?;
        let first = *b.get(1)? as usize;
        if first < 0x80 {
            return Some((tag, 2, first));
        }
        let n = first & 0x7F;
        if n == 0 || n > 4 {
            return None;
        }
        let len = b
            .get(2..2 + n)?
            .iter()
            .fold(0usize, |a, x| a << 8 | *x as usize);
        Some((tag, 2 + n, len))
    }
    fn inside(b: &[u8]) -> Option<&[u8]> {
        let (_, h, l) = tlv(b)?;
        b.get(h..h + l)
    }
    fn skip(b: &[u8]) -> Option<usize> {
        let (_, h, l) = tlv(b)?;
        Some(h + l)
    }
    let tbs = inside(inside(cert)?)?;
    let mut at = 0;
    if tbs.first() == Some(&0xA0) {
        at += skip(tbs)?; // [0] version
    }
    for _ in 0..5 {
        // serial, signature algorithm, issuer, validity, subject
        at += skip(tbs.get(at..)?)?;
    }
    let rest = tbs.get(at..)?;
    let (tag, h, l) = tlv(rest)?;
    (tag == 0x30).then(|| rest.get(..h + l)).flatten()
}

#[derive(Debug)]
struct Pinned {
    pin: [u8; 32],
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let key = spki(end_entity).ok_or(rustls::Error::InvalidCertificate(
            CertificateError::BadEncoding,
        ))?;
        if crate::crypto::same(&sha256(key), &self.pin) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

impl Transport for rustls::StreamOwned<rustls::ClientConnection, TcpStream> {
    fn set_read_timeout(&mut self, t: Option<Duration>) -> std::io::Result<()> {
        self.sock.set_read_timeout(t)
    }
}

/// A connector for [`crate::GameServer::tls`]: TLS to the URL's address, `sni` as the server
/// name, the server's key pinned to `pin` (SHA-256 of its SubjectPublicKeyInfo).
pub fn pinned(sni: &str, pin: [u8; 32]) -> Result<Connector, Error> {
    let provider = Arc::new(ring::default_provider());
    let mut config = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::Protocol(e.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned { pin, provider }))
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let config = Arc::new(config);
    let name = ServerName::try_from(sni.to_string())
        .map_err(|_| Error::Protocol(format!("not a server name: {sni}")))?;
    Ok(Arc::new(move |host: &str, port: u16, timeout: Duration| {
        let addr = (host, port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| Error::Protocol(format!("{host}: no address")))?;
        let tcp = TcpStream::connect_timeout(&addr, timeout)?;
        tcp.set_nodelay(true)?;
        tcp.set_read_timeout(Some(timeout))?;
        tcp.set_write_timeout(Some(timeout))?;
        let conn = rustls::ClientConnection::new(config.clone(), name.clone())
            .map_err(|e| Error::Protocol(e.to_string()))?;
        let mut s = rustls::StreamOwned::new(conn, tcp);
        while s.conn.is_handshaking() {
            s.conn
                .complete_io(&mut s.sock)
                .map_err(|e| Error::Protocol(format!("TLS: {e}")))?;
        }
        Ok(Box::new(s) as Box<dyn Transport>)
    }))
}

/// A pin given as 64 hex digits.
pub fn parse_pin(hex: &str) -> Option<[u8; 32]> {
    crate::crypto::unhex(hex.trim())?.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spki_walks_a_certificate() {
        // a minimal certificate shape: SEQ { SEQ tbs { [0]{INT 2}, INT, SEQ, SEQ, SEQ, SEQ,
        // SEQ spki { … } }, … }
        let spki_el = [0x30, 0x03, 0x02, 0x01, 0x07];
        let mut tbs = vec![0xA0, 0x03, 0x02, 0x01, 0x02, 0x02, 0x01, 0x01];
        for _ in 0..4 {
            tbs.extend_from_slice(&[0x30, 0x00]);
        }
        tbs.extend_from_slice(&spki_el);
        tbs.extend_from_slice(&[0xA3, 0x00]);
        let mut tbs_el = vec![0x30, tbs.len() as u8];
        tbs_el.extend(tbs);
        let mut cert = vec![0x30, 0x81, (tbs_el.len() + 2) as u8];
        cert.extend(tbs_el);
        cert.extend_from_slice(&[0x30, 0x00]);
        assert_eq!(spki(&cert), Some(&spki_el[..]));
        assert_eq!(spki(&[0x30, 0x05]), None);
        assert!(parse_pin(&"ab".repeat(32)).is_some());
        assert!(parse_pin("abc").is_none());
    }
}
