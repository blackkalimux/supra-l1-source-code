//! This module provides functionality for managing NiDKG (Non-Interactive Distributed Key Generation) dealings and related cryptographic operations.
//! It includes functions for generating centralized classgroup encryption/decryption keys, creating dealings, computing transcripts, and generating BLS keys.
use crate::common::error::CliError;
use encryption::{
    decrypt, encrypt, new_password_with_policy, read_password, NEW_PASSWORD_INPUT_MESSAGE,
};
use file_io_derive::HomeFileIo;
use nidkg_helper::cgdkg::nidkg::NiCGDkg;
use nidkg_helper::cgdkg::{
    combine_public_keys_ecp2_12381, combine_public_keys_ecp2_254, combine_public_keys_ecp_12381,
    combine_public_keys_ecp_254,
};
use nidkg_helper::errors::DkgError::AggregationError;
use nidkg_helper::{
    BlsPrivateKey, BlsPublicKey, DLEqProof, PublicKeyBls12381, PublicKeyBn254, PublicPolynomial,
};
use serde::{Deserialize, Serialize};
use sodkg::{CGPublicKey, CGSecretKey, Dealing};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};

/// Private key of the class group encryption.
#[derive(Default, Clone, Serialize, Deserialize, HomeFileIo, Debug)]
#[home_filename = "dkg_private_key.pem"]
pub struct DkgCGPrivateKey {
    key: String,
}

impl From<&CGSecretKey> for DkgCGPrivateKey {
    fn from(priv_key: &CGSecretKey) -> Self {
        DkgCGPrivateKey {
            key: hex::encode(priv_key.to_vec()),
        }
    }
}

impl TryFrom<&DkgCGPrivateKey> for CGSecretKey {
    type Error = CliError;

    fn try_from(dkg_key: &DkgCGPrivateKey) -> Result<Self, Self::Error> {
        let cg_s_bytes = hex::decode(&dkg_key.key).map_err(CliError::HexDecodeError)?;
        let cg_s_key = CGSecretKey::try_from(cg_s_bytes.as_slice()).map_err(CliError::DkgError)?;
        Ok(cg_s_key)
    }
}

impl DkgCGPrivateKey {
    pub fn encrypt(&self, password: &str) -> Result<Vec<u8>, CliError> {
        let serialized = serde_json::to_vec(self)?;
        if let Ok(encrypted) = encrypt(&serialized, password) {
            Ok(encrypted)
        } else {
            Err(CliError::GeneralError("Encryption failed".to_owned()))
        }
    }

    pub fn decrypt(data: &[u8], password: &str) -> Result<Self, CliError> {
        if let Ok(decrypted_data) = decrypt(data, password) {
            let priv_key = serde_json::from_slice(&decrypted_data)?;
            Ok(priv_key)
        } else {
            Err(CliError::GeneralError(
                "Decryption failed. Check the password again".to_owned(),
            ))
        }
    }

    pub fn update(&self, password: &str) -> Result<(), CliError> {
        let result = self.encrypt(password)?;
        let mut file = File::create(Self::default_path()?)?;
        file.write_all(&result)?;
        Ok(())
    }

    pub fn read() -> Result<Self, CliError> {
        let dkg_private_key = match (
            Self::default_path()?.parent(),
            Self::default_path()?.exists(),
        ) {
            (_, true) => {
                println!("Enter your password for CG private key: ");
                let password = read_password().map_err(CliError::StdIoError)?;
                let mut file = File::open(Self::default_path()?)?;
                let mut encrypted_data = Vec::new();
                file.read_to_end(&mut encrypted_data)?;
                DkgCGPrivateKey::decrypt(&encrypted_data, &password)?
            }
            (_, _) => {
                return Err(CliError::Aborted(
                    format!("cannot find path {:?}", Self::default_path()?.parent()),
                    "create the directory at required path".to_string(),
                ))
            }
        };
        Ok(dkg_private_key)
    }
}

/// Public key of the class group encryption.
#[derive(Default, Clone, Serialize, Deserialize, HomeFileIo, Debug)]
#[home_filename = "dkg_public_key.json"]
pub struct DkgCGPublicKey {
    key: String,
}

impl From<&CGPublicKey> for DkgCGPublicKey {
    fn from(pub_key: &CGPublicKey) -> Self {
        DkgCGPublicKey {
            key: hex::encode(pub_key.to_vec()),
        }
    }
}

impl TryFrom<&DkgCGPublicKey> for CGPublicKey {
    type Error = CliError;

    fn try_from(dkg_key: &DkgCGPublicKey) -> Result<Self, Self::Error> {
        let cg_s_bytes = hex::decode(&dkg_key.key).map_err(CliError::HexDecodeError)?;
        let cg_s_key = CGPublicKey::try_from(cg_s_bytes.as_slice()).map_err(CliError::DkgError)?;
        Ok(cg_s_key)
    }
}

impl DkgCGPublicKey {
    pub fn update(&self) -> Result<(), CliError> {
        Ok(self.write_to_file(Self::default_path()?)?)
    }
    pub fn read() -> Result<Self, CliError> {
        let node_cg_pub_key = match (
            Self::default_path()?.parent(),
            Self::default_path()?.exists(),
        ) {
            (_, true) => DkgCGPublicKey::read_from_file(Self::default_path()?)?,
            (_, _) => {
                return Err(CliError::Aborted(
                    format!("cannot find path {:?}", Self::default_path()?.parent()),
                    "create the directory at required path".to_string(),
                ))
            }
        };
        Ok(node_cg_pub_key)
    }
}

/// BLS private key.
#[derive(HomeFileIo, Serialize, Deserialize)]
#[home_filename = "dkg_bls_private_key.pem"]
pub struct DkgBlsPrivKey {
    key: String,
}

impl From<&BlsPrivateKey> for DkgBlsPrivKey {
    fn from(priv_key: &BlsPrivateKey) -> Self {
        DkgBlsPrivKey {
            key: hex::encode(priv_key.to_vec()),
        }
    }
}

impl TryFrom<&DkgBlsPrivKey> for BlsPrivateKey {
    type Error = CliError;

    fn try_from(dkg_key: &DkgBlsPrivKey) -> Result<Self, Self::Error> {
        let cg_s_bytes = hex::decode(&dkg_key.key).map_err(CliError::HexDecodeError)?;
        let cg_bls_key = BlsPrivateKey::try_from(cg_s_bytes.as_slice())
            .map_err(|e| CliError::GeneralError(e.to_string()))?;
        Ok(cg_bls_key)
    }
}

impl DkgBlsPrivKey {
    /// Encrypts the BLS private key with a password.
    pub fn encrypt(&self, password: &str) -> Result<Vec<u8>, CliError> {
        let serialized = serde_json::to_vec(self)?;
        if let Ok(encrypted) = encrypt(&serialized, password) {
            Ok(encrypted)
        } else {
            Err(CliError::GeneralError("Encryption failed".to_owned()))
        }
    }

    /// Decrypts the BLS private key using a password.
    pub fn decrypt(data: &[u8], password: &str) -> Result<Self, CliError> {
        if let Ok(decrypted_data) = decrypt(data, password) {
            let bls_piv_key = serde_json::from_slice(&decrypted_data)?;
            Ok(bls_piv_key)
        } else {
            Err(CliError::GeneralError(
                "Decryption failed. Check the password again".to_owned(),
            ))
        }
    }

    pub fn update(&self, password: &str) -> Result<(), CliError> {
        let result = self.encrypt(password)?;
        let mut file = File::create(Self::default_path()?)?;
        file.write_all(&result)?;
        Ok(())
    }

    pub fn read() -> Result<Self, CliError> {
        let dkg_bls_private_key = match (
            Self::default_path()?.parent(),
            Self::default_path()?.exists(),
        ) {
            (_, true) => {
                println!("Enter your password for BLS private key: ");
                let password = read_password().map_err(CliError::StdIoError)?;
                let mut file = File::open(Self::default_path()?)?;
                let mut encrypted_data = Vec::new();
                file.read_to_end(&mut encrypted_data)?;
                DkgBlsPrivKey::decrypt(&encrypted_data, &password)?
            }
            (_, _) => {
                return Err(CliError::Aborted(
                    format!("cannot find path {:?}", Self::default_path()?.parent()),
                    "create the directory at required path".to_string(),
                ))
            }
        };
        Ok(dkg_bls_private_key)
    }
}

/// Dkg BLS Public Key Share, Aggregate Public Polynomial, DLEQ proof which is output of the NiDKG process.
#[derive(Clone, Debug)]
pub struct DkgBlsPubKeyShareAndProof {
    bls_pub_key: BlsPublicKey,
    proof: DLEqProof,
    public_poly: PublicPolynomial,
}

/// Dkg Bls Public Key Share, Aggregate Public Polynomial, dleq proof which is output of the NiDKG process.
#[derive(Default, Clone, HomeFileIo, Serialize, Deserialize, Debug, PartialEq)]
#[home_filename = "dkg_bls_public_key.json"]
pub struct DkgBlsPubKeyShareAndProofBytes {
    bls_pub_key: String,
    proof: String,
    public_poly: String,
}

impl From<&DkgBlsPubKeyShareAndProof> for DkgBlsPubKeyShareAndProofBytes {
    fn from(pub_key_and_proof: &DkgBlsPubKeyShareAndProof) -> Self {
        let bls_key = pub_key_and_proof.bls_pub_key.to_vec();
        let zkproof_dleq = pub_key_and_proof.proof.to_vec();
        let pub_coeff = pub_key_and_proof.clone().public_poly.into_bytes();

        DkgBlsPubKeyShareAndProofBytes {
            bls_pub_key: hex::encode(bls_key),
            proof: hex::encode(zkproof_dleq),
            public_poly: hex::encode(pub_coeff),
        }
    }
}

impl TryFrom<&DkgBlsPubKeyShareAndProofBytes> for DkgBlsPubKeyShareAndProof {
    type Error = CliError;

    fn try_from(
        pub_key_and_proof_bytes: &DkgBlsPubKeyShareAndProofBytes,
    ) -> Result<Self, Self::Error> {
        let bls_key_arr: &[u8] =
            &hex::decode(&pub_key_and_proof_bytes.bls_pub_key).map_err(CliError::HexDecodeError)?;
        let bls_key = BlsPublicKey::try_from(bls_key_arr).map_err(CliError::DkgError)?;

        let dleq_proof_arr: &[u8] =
            &hex::decode(&pub_key_and_proof_bytes.proof).map_err(CliError::HexDecodeError)?;
        let dleq_proof = DLEqProof::try_from(dleq_proof_arr).map_err(CliError::DkgError)?;

        let pub_poly: &[u8] =
            &hex::decode(&pub_key_and_proof_bytes.public_poly).map_err(CliError::HexDecodeError)?;
        let pub_coeff = PublicPolynomial::try_from(pub_poly).map_err(CliError::DkgError)?;

        Ok(DkgBlsPubKeyShareAndProof {
            bls_pub_key: bls_key,
            proof: dleq_proof,
            public_poly: pub_coeff,
        })
    }
}

impl DkgBlsPubKeyShareAndProofBytes {
    pub fn update(&self) -> Result<(), CliError> {
        Ok(self.write_to_default_path()?)
    }
    pub fn read() -> Result<Self, CliError> {
        let dkg_bls_public_key = match (
            Self::default_path()?.parent(),
            Self::default_path()?.exists(),
        ) {
            (_, true) => DkgBlsPubKeyShareAndProofBytes::read_from_file(Self::default_path()?)?,
            (_, _) => {
                return Err(CliError::Aborted(
                    format!("cannot find path {:?}", Self::default_path()?.parent()),
                    "create the directory at required path".to_string(),
                ))
            }
        };
        Ok(dkg_bls_public_key)
    }
}

/// Dkg Bls Public Key Transcript containing Bls Public Key shares of the committee nodes
/// This transcript is used to generate the committee BLS public key
#[derive(Default, Clone, HomeFileIo, Serialize, Deserialize, Debug)]
#[home_filename = "dkg_bls_pubkey_transcript.json"]
pub struct DkgBlsPubKeyTranscript {
    bls_pub_key_shares: BTreeMap<u32, DkgBlsPubKeyShareAndProofBytes>,
}

impl DkgBlsPubKeyTranscript {
    pub fn update(&self) -> Result<(), CliError> {
        Ok(self.write_to_default_path()?)
    }
    pub fn read() -> Result<Self, CliError> {
        let dkg_bls_public_key_transcript = match (
            Self::default_path()?.parent(),
            Self::default_path()?.exists(),
        ) {
            (_, true) => DkgBlsPubKeyTranscript::read_from_file(Self::default_path()?)?,
            (_, _) => {
                return Err(CliError::Aborted(
                    format!("cannot find path {:?}", Self::default_path()?.parent()),
                    "create the directory at required path".to_string(),
                ))
            }
        };
        Ok(dkg_bls_public_key_transcript)
    }
}

/// Committee BLS Public key

#[derive(Clone, Debug)]
pub struct DkgCommitteeBLSPublicKey {
    bls_pub_key: BlsPublicKey,
    public_poly: PublicPolynomial,
}

#[derive(Default, Clone, HomeFileIo, Serialize, Deserialize, Debug)]
#[home_filename = "dkg_committee_bls_public_key.json"]
pub struct DkgCommitteeBLSPublicKeyBytes {
    bls_pub_key: String,
    public_poly: String,
}

impl From<&DkgCommitteeBLSPublicKey> for DkgCommitteeBLSPublicKeyBytes {
    fn from(public_key: &DkgCommitteeBLSPublicKey) -> Self {
        let bls_key = public_key.bls_pub_key.to_vec();
        let pub_coeff = public_key.clone().public_poly.into_bytes();
        DkgCommitteeBLSPublicKeyBytes {
            bls_pub_key: hex::encode(bls_key),
            public_poly: hex::encode(pub_coeff),
        }
    }
}

impl TryFrom<&DkgCommitteeBLSPublicKeyBytes> for DkgCommitteeBLSPublicKey {
    type Error = CliError;

    fn try_from(
        pub_key_and_proof_bytes: &DkgCommitteeBLSPublicKeyBytes,
    ) -> Result<Self, Self::Error> {
        let bls_key_arr: &[u8] =
            &hex::decode(&pub_key_and_proof_bytes.bls_pub_key).map_err(CliError::HexDecodeError)?;
        let bls_key = BlsPublicKey::try_from(bls_key_arr).map_err(CliError::DkgError)?;

        let pub_poly: &[u8] =
            &hex::decode(&pub_key_and_proof_bytes.public_poly).map_err(CliError::HexDecodeError)?;
        let pub_coeff = PublicPolynomial::try_from(pub_poly).map_err(CliError::DkgError)?;

        Ok(DkgCommitteeBLSPublicKey {
            bls_pub_key: bls_key,
            public_poly: pub_coeff,
        })
    }
}

impl DkgCommitteeBLSPublicKeyBytes {
    pub fn update(&self) -> Result<(), CliError> {
        Ok(self.write_to_default_path()?)
    }
    pub fn read() -> Result<Self, CliError> {
        let dkg_committee_bls_key = match (
            Self::default_path()?.parent(),
            Self::default_path()?.exists(),
        ) {
            (_, true) => DkgCommitteeBLSPublicKeyBytes::read_from_file(Self::default_path()?)?,
            (_, _) => {
                return Err(CliError::Aborted(
                    format!("cannot find path {:?}", Self::default_path()?.parent()),
                    "create the directory at required path".to_string(),
                ))
            }
        };
        Ok(dkg_committee_bls_key)
    }
}

/// Configuration of the NiDKG process.
#[derive(Default, HomeFileIo, Serialize, Deserialize)]
#[home_filename = "dkg_config.json"]
pub struct DkgConfig {
    threshold: u32,
    nodes: u32,
    cg_public_keys: BTreeMap<u32, DkgCGPublicKey>,
}

impl DkgConfig {
    pub fn update(&self) -> Result<(), CliError> {
        Ok(self.write_to_file(Self::default_path()?)?)
    }
    pub fn read() -> Result<Self, CliError> {
        let dkg_config = match (
            Self::default_path()?.parent(),
            Self::default_path()?.exists(),
        ) {
            (_, true) => DkgConfig::read_from_file(Self::default_path()?)?,
            (_, _) => {
                return Err(CliError::Aborted(
                    format!("cannot find path {:?}", Self::default_path()?.parent()),
                    "create the directory at required path".to_string(),
                ))
            }
        };
        Ok(dkg_config)
    }

    pub fn node_index_for_pk(&self, public_key: &DkgCGPublicKey) -> Result<u32, CliError> {
        for (node_index, pk) in self.cg_public_keys.iter() {
            if public_key.key == pk.key {
                return Ok(*node_index);
            }
        }
        Err(CliError::GeneralError(
            "Node's CG public key does not correspond to any key in Dkg Config".to_owned(),
        ))
    }

    pub fn get_pks(&self) -> Result<BTreeMap<u32, CGPublicKey>, CliError> {
        let mut cg_pks: BTreeMap<u32, CGPublicKey> = BTreeMap::new();
        for (node_index, key_ser) in &self.cg_public_keys {
            let cg_pub_key = CGPublicKey::try_from(key_ser)?;
            cg_pks.insert(*node_index, cg_pub_key);
        }
        Ok(cg_pks)
    }
}

/// Dealing in the NiDKG process.
#[derive(Default, Clone, Serialize, Deserialize, HomeFileIo, Debug)]
#[home_filename = "dkg_dealing.json"]
pub struct DkgDealing {
    dealing: String,
}

impl From<&Dealing> for DkgDealing {
    fn from(dealing: &Dealing) -> Self {
        DkgDealing {
            dealing: hex::encode(dealing.to_vec()), //encoded dealing.
        }
    }
}

impl TryFrom<&DkgDealing> for Dealing {
    type Error = CliError;

    fn try_from(dkg_key: &DkgDealing) -> Result<Self, Self::Error> {
        let dealing_bytes = hex::decode(&dkg_key.dealing).map_err(CliError::HexDecodeError)?;
        let dealing = Dealing::try_from(dealing_bytes.as_slice()).map_err(CliError::DkgError)?;
        Ok(dealing)
    }
}

impl DkgDealing {
    pub fn update(&self) -> Result<(), CliError> {
        Ok(self.write_to_file(Self::default_path()?)?)
    }

    pub fn read() -> Result<Self, CliError> {
        let node_dealing = match (
            Self::default_path()?.parent(),
            Self::default_path()?.exists(),
        ) {
            (_, true) => DkgDealing::read_from_file(Self::default_path()?)?,
            (_, _) => {
                return Err(CliError::Aborted(
                    format!("cannot find path {:?}", Self::default_path()?.parent()),
                    "create the directory at required path".to_string(),
                ))
            }
        };
        Ok(node_dealing)
    }
}

/// Transcript of the NiDKG process.
#[derive(Default, HomeFileIo, Serialize, Deserialize)]
#[home_filename = "dkg_transcript.json"]
pub struct DkgTranscript {
    dealings: BTreeMap<u32, DkgDealing>,
}

impl DkgTranscript {
    pub fn update(&self) -> Result<(), CliError> {
        Ok(self.write_to_default_path()?)
    }
    pub fn read() -> Result<Self, CliError> {
        let dkg_transcript = match (
            Self::default_path()?.parent(),
            Self::default_path()?.exists(),
        ) {
            (_, true) => DkgTranscript::read_from_file(Self::default_path()?)?,
            (Some(config_dir), false) => {
                std::fs::create_dir_all(config_dir).map_err(CliError::StdIoError)?;
                let dkg_transcript = DkgTranscript::default();
                dkg_transcript.update()?;
                dkg_transcript
            }
            (_, _) => {
                return Err(CliError::Aborted(
                    format!("cannot find path {:?}", Self::default_path()?.parent()),
                    "create the directory at required path".to_string(),
                ))
            }
        };
        Ok(dkg_transcript)
    }
}

/// Generates a single class group encryption/decryption key pair
///
/// This function generates a class group encryption/decryption key pair, for a node.
/// The public and private keys are stored in separate files named after node position.
///
/// # Returns
///
/// Writes the generated key pair to the appropriate files.
///
pub fn generate_cg_encryption_key_pair() -> Result<(), CliError> {
    // PRIVATE: private keys should only be for each node
    let cg_priv_key = CGSecretKey::generate();
    let node_priv_key = DkgCGPrivateKey::from(&cg_priv_key);
    println!("Generating CG private key");
    if let Ok(password) = new_password_with_policy(NEW_PASSWORD_INPUT_MESSAGE.to_string()) {
        node_priv_key.update(&password)?;
        // PUBLIC: public keys and proof of possession should be available with everyone
        let cg_pub_key = CGPublicKey::try_from(&cg_priv_key)?;
        let node_pub_key = DkgCGPublicKey::from(&cg_pub_key);
        node_pub_key.update()?;
        Ok(())
    } else {
        Err(CliError::GeneralError("Unable to read password".to_owned()))
    }
}

/// Generates a dealing for this node in the NiDKG process.
///
/// This function reads the DKG configuration file to retrieve public keys of all nodes, then uses them to generate a dealing for this node.
/// The generated dealing is then stored in a dealing file.
///
/// # Returns
///
/// Writes the generated dealing to the file.
///
pub fn generate_dealing() -> Result<(), CliError> {
    let dkg_config = DkgConfig::read()?;
    let cg_pks = dkg_config.get_pks()?;
    let dealing = NiCGDkg::generate_dealing(dkg_config.threshold, dkg_config.nodes, &cg_pks, None);
    let dealing_ser = DkgDealing::from(&dealing);
    dealing_ser.update()?;
    Ok(())
}

/// Generates a resharing dealing for this node in the NiDKG process.
///
/// This function reads the DKG configuration file to retrieve public keys of all nodes and bls_private_key file to retrieve node's bls key, then uses them to generate a resharing dealing for this node.
/// The generated dealing is then stored in a dealing file and overrides if there is existing dealing for this node.
///
/// # Returns
///
/// Writes the generated dealing to the file.
///
pub fn generate_resharing_dealing() -> Result<(), CliError> {
    let dkg_config = DkgConfig::read()?;
    let cg_pks = dkg_config.get_pks()?;
    let bls_private_key_bytes = DkgBlsPrivKey::read()?;
    let bls_private_key = BlsPrivateKey::try_from(&bls_private_key_bytes)?;
    let dealing = NiCGDkg::generate_dealing(
        dkg_config.threshold,
        dkg_config.nodes,
        &cg_pks,
        Some(bls_private_key),
    );
    let node_dealing = DkgDealing::from(&dealing);
    node_dealing.update()?;
    Ok(())
}

/// Generates BLS keys for this node using the transcript, node's cg private key, and public key.
///
/// This function reads the DKG transcript, retrieves the node's private and public keys from the key files, and uses them to generate BLS keypair (private and public key).
/// The BLS private key is encrypted with a password provided by the user and stored in a designated file. The public key is stored in a separate file.
///
/// # Returns
///
/// Generates and stores the BLS keypair for the node.
/// If password input fails, a `GeneralError` is returned. If the transcript has insufficient dealings, a `DKGTranscriptError` is returned.
///
pub fn generate_key() -> Result<(), CliError> {
    let dkg_config = DkgConfig::read()?;
    let cg_pks = dkg_config.get_pks()?;
    let transcript = DkgTranscript::read()?;
    let cg_private_key_ser = DkgCGPrivateKey::read()?;
    let cg_private_key = CGSecretKey::try_from(&cg_private_key_ser)?;
    let cg_pub_key = DkgCGPublicKey::read()?;

    let node_index = dkg_config.node_index_for_pk(&cg_pub_key)?;

    let mut dealings = Vec::new();
    for dealing_ser in transcript.dealings.values() {
        let node_dealing = Dealing::try_from(dealing_ser)?;
        if NiCGDkg::verify_dealing(
            &node_dealing,
            &cg_pks,
            dkg_config.nodes,
            dkg_config.threshold,
        ) {
            dealings.push(node_dealing);
        }

        if dealings.len() == dkg_config.threshold as usize {
            break;
        }
    }

    if dealings.len() < dkg_config.threshold as usize {
        return Err(CliError::DKGTranscriptError(
            dkg_config.threshold,
            dealings.len(),
        ));
    }

    let dealings_refs: Vec<&Dealing> = dealings.iter().collect();

    let (bls_key, bls_pubkey, dleqproof, public_poly) = NiCGDkg::aggregate_dealings_for_self_group(
        &dealings_refs,
        &cg_private_key,
        node_index as usize,
    )
    .map_err(|e| CliError::DkgError(AggregationError(e.to_string())))?;

    let bls_priv_key = DkgBlsPrivKey::from(&bls_key);

    println!("Creating BLS private key");
    if let Ok(password) = new_password_with_policy(NEW_PASSWORD_INPUT_MESSAGE.to_string()) {
        bls_priv_key.update(&password)?;
        let bls_pub_key_and_proof = DkgBlsPubKeyShareAndProof {
            bls_pub_key: bls_pubkey,
            proof: dleqproof,
            public_poly,
        };
        let bls_pub_key_and_proof_bytes =
            DkgBlsPubKeyShareAndProofBytes::from(&bls_pub_key_and_proof);
        bls_pub_key_and_proof_bytes.update()?;
        Ok(())
    } else {
        Err(CliError::GeneralError("Unable to read password".to_owned()))
    }
}

/// Generates BLS keys for this node using the reshared transcript, aggregated public polynomial and the node's private key.
///
/// This function reads the reshared DKG transcript, the node's private key, and aggregate public polynomial and uses them to generate BLS keypair (private and public key).
/// The BLS private key is encrypted with a password provided by the user and stored in a designated file. The public key is stored in a separate file.
///
/// # Returns
///
/// Generates and stores the BLS keypair for the node.
/// If password input fails, a `GeneralError` is returned. If the transcript has insufficient dealings, a `DKGTranscriptError` is returned.
///
pub fn generate_reshare_key() -> Result<(), CliError> {
    let cg_private_key_ser = DkgCGPrivateKey::read()?;
    let cg_private_key = CGSecretKey::try_from(&cg_private_key_ser)?;
    let transcript = DkgTranscript::read()?;
    let dkg_config = DkgConfig::read()?;
    let cg_pks = dkg_config.get_pks()?;
    let committee_publickey_and_proof_ser = DkgCommitteeBLSPublicKeyBytes::read()?;
    let reshared_public_poly =
        DkgCommitteeBLSPublicKey::try_from(&committee_publickey_and_proof_ser)?.public_poly;
    let cg_pub_key = DkgCGPublicKey::read()?;
    let node_index = dkg_config.node_index_for_pk(&cg_pub_key)?;

    let mut dealings = BTreeMap::new();
    for (node_index, dealing_ser) in &transcript.dealings {
        let node_dealing = Dealing::try_from(dealing_ser)?;
        if NiCGDkg::verify_resharing_dealing(
            &node_dealing,
            &cg_pks,
            dkg_config.nodes,
            dkg_config.threshold,
            *node_index,
            &reshared_public_poly,
        ) {
            dealings.insert(*node_index, node_dealing);
        }

        if dealings.len() == dkg_config.threshold as usize {
            break;
        }
    }

    if dealings.len() < dkg_config.threshold as usize {
        return Err(CliError::DKGTranscriptError(
            dkg_config.threshold,
            dealings.len(),
        ));
    }

    let (bls_key, bls_pubkey, dleqproof, public_poly) =
        NiCGDkg::aggregate_resharing_dealings_for_self_group(
            &dealings,
            &cg_private_key,
            node_index,
            dkg_config.threshold,
            &reshared_public_poly,
        )
        .map_err(|e| CliError::DkgError(AggregationError(e.to_string())))?;

    let bls_priv_key = DkgBlsPrivKey::from(&bls_key);
    println!("Creating BLS private key");
    if let Ok(password) = new_password_with_policy(NEW_PASSWORD_INPUT_MESSAGE.to_string()) {
        bls_priv_key.update(&password)?;
        let bls_pub_key_and_proof = DkgBlsPubKeyShareAndProof {
            bls_pub_key: bls_pubkey,
            proof: dleqproof,
            public_poly,
        };
        let bls_pub_key_and_proof_bytes =
            DkgBlsPubKeyShareAndProofBytes::from(&bls_pub_key_and_proof);
        bls_pub_key_and_proof_bytes.update()?;
        Ok(())
    } else {
        Err(CliError::GeneralError("Unable to read password".to_owned()))
    }
}

/// Generates Committee Threshold BLS key using the BLS keys transcript
///
/// This function reads the DKG Bls public key transcript file, verifies each public key share and uses them to generate committee threshold BLS public key.
///
/// # Returns
///
/// Generates and stores the committee threshold BLS public key.
/// If the transcript file doesnot have enough public key shares or there is some error while generating the committee key a `GeneralError` is returned.
///
pub fn generate_committee_key() -> Result<(), CliError> {
    let dkg_config = DkgConfig::read()?;
    let bls_pub_key_transcript = DkgBlsPubKeyTranscript::read()?;
    let bls_pub_key = DkgBlsPubKeyShareAndProofBytes::read()?;
    let mut node_bls_pub_keys_12381_ecp = Vec::new();
    let mut node_bls_pub_keys_254_ecp = Vec::new();
    let mut node_bls_pub_keys_12381_ecp2 = Vec::new();
    let mut node_bls_pub_keys_254_ecp2 = Vec::new();
    let mut pub_polys_12381 = Vec::new();
    let mut pub_polys_254 = Vec::new();

    //The BLS public key transcript file should contain the node's own bls public key share
    if !bls_pub_key_transcript
        .bls_pub_key_shares
        .values()
        .any(|key| *key == bls_pub_key)
    {
        return Err(CliError::GeneralError(
            "Invalid Bls Public Key Transcript file. Make sure you are not using an outdated file."
                .to_string(),
        ));
    }

    for i in 0..dkg_config.nodes {
        let transcript_contains_key = bls_pub_key_transcript.bls_pub_key_shares.contains_key(&i);
        let mut public_key_is_valid = false;

        if transcript_contains_key {
            let bls_public_key_and_proof = DkgBlsPubKeyShareAndProof::try_from(
                bls_pub_key_transcript
                    .bls_pub_key_shares
                    .get(&i)
                    .expect("missing bls_pub_key_shares"),
            )?;
            public_key_is_valid = NiCGDkg::verify_public_keys_and_proof12381(
                i as u64,
                &bls_public_key_and_proof.bls_pub_key.bls12381.pub_key_g1,
                &bls_public_key_and_proof.bls_pub_key.bls12381.pub_key_g2,
                &bls_public_key_and_proof.proof.nizk_12381,
                &bls_public_key_and_proof.public_poly.polynomial_bls12381,
            ) && NiCGDkg::verify_public_keys_and_proof254(
                i as u64,
                &bls_public_key_and_proof.bls_pub_key.bn254.pub_key_g1,
                &bls_public_key_and_proof.bls_pub_key.bn254.pub_key_g2,
                &bls_public_key_and_proof.proof.nizk_254,
                &bls_public_key_and_proof.public_poly.polynomial_bn254,
            );

            if public_key_is_valid {
                node_bls_pub_keys_12381_ecp.push(Some(
                    bls_public_key_and_proof.bls_pub_key.bls12381.pub_key_g1,
                ));
                node_bls_pub_keys_12381_ecp2.push(Some(
                    bls_public_key_and_proof.bls_pub_key.bls12381.pub_key_g2,
                ));
                node_bls_pub_keys_254_ecp
                    .push(Some(bls_public_key_and_proof.bls_pub_key.bn254.pub_key_g1));
                node_bls_pub_keys_254_ecp2
                    .push(Some(bls_public_key_and_proof.bls_pub_key.bn254.pub_key_g2));
                pub_polys_12381.push(bls_public_key_and_proof.public_poly.polynomial_bls12381);
                pub_polys_254.push(bls_public_key_and_proof.public_poly.polynomial_bn254);
            }
        }

        if !transcript_contains_key || !public_key_is_valid {
            node_bls_pub_keys_12381_ecp.push(None);
            node_bls_pub_keys_254_ecp.push(None);
            node_bls_pub_keys_12381_ecp2.push(None);
            node_bls_pub_keys_254_ecp2.push(None);
        }
    }

    // if we do not have >= threshold bls public key shares, a committee key cannot be generated
    if node_bls_pub_keys_12381_ecp
        .iter()
        .filter(|s| s.is_some())
        .count()
        < dkg_config.threshold as usize
    {
        return Err(CliError::GeneralError(
            format!(
                "Not enough valid BLS public key shares present to generate committee key. Expected shares: {}, Found: {}",
                dkg_config.threshold, node_bls_pub_keys_12381_ecp.iter().filter(|s| s.is_some()).count()
            )));
    }

    let committee_pk_ecp_12381_result = combine_public_keys_ecp_12381(
        node_bls_pub_keys_12381_ecp.as_slice(),
        dkg_config.threshold as usize,
    );
    let committee_pk_ecp2_12381_result = combine_public_keys_ecp2_12381(
        node_bls_pub_keys_12381_ecp2.as_slice(),
        dkg_config.threshold as usize,
    );
    let committee_pk_ecp_254_result = combine_public_keys_ecp_254(
        node_bls_pub_keys_254_ecp.as_slice(),
        dkg_config.threshold as usize,
    );
    let committee_pk_ecp2_254_result = combine_public_keys_ecp2_254(
        node_bls_pub_keys_254_ecp2.as_slice(),
        dkg_config.threshold as usize,
    );

    // if committee_pk_ecp_12381_result.is_ok()
    //     && committee_pk_ecp2_12381_result.is_ok()
    //     && committee_pk_ecp_254_result.is_ok()
    //     && committee_pk_ecp2_254_result.is_ok()
    if let (
        Ok(committee_pk_ecp_12381),
        Ok(committee_pk_ecp2_12381),
        Ok(committee_pk_ecp_254),
        Ok(committee_pk_ecp2_254),
    ) = (
        committee_pk_ecp_12381_result.clone(),
        committee_pk_ecp2_12381_result,
        committee_pk_ecp_254_result,
        committee_pk_ecp2_254_result,
    ) {
        let committee_bls_key = BlsPublicKey {
            bls12381: PublicKeyBls12381 {
                pub_key_g1: committee_pk_ecp_12381,
                pub_key_g2: committee_pk_ecp2_12381,
            },
            bn254: PublicKeyBn254 {
                pub_key_g1: committee_pk_ecp_254,
                pub_key_g2: committee_pk_ecp2_254,
            },
        };

        if !pub_polys_12381
            .iter()
            .all(|poly| poly == &pub_polys_12381[0])
            || !pub_polys_254.iter().all(|poly| poly == &pub_polys_254[0])
        {
            return Err(CliError::GeneralError(
                "Invalid public polynomials.".to_owned(),
            ));
        }

        let committee_bls_key_and_pubpoly = DkgCommitteeBLSPublicKey {
            bls_pub_key: committee_bls_key,
            public_poly: PublicPolynomial {
                polynomial_bls12381: pub_polys_12381[0].clone(),
                polynomial_bn254: pub_polys_254[0].clone(),
            },
        };

        let committee_bls_key_ser =
            DkgCommitteeBLSPublicKeyBytes::from(&committee_bls_key_and_pubpoly);
        committee_bls_key_ser.update()?;
        Ok(())
    } else {
        Err(CliError::GeneralError(
            committee_pk_ecp_12381_result
                .expect_err("committee_pk_ecp_12381_result should fail")
                .message,
        ))
    }
}

/// Outputs Committee Threshold BLS keys computed by this node.
///
/// This function reads the dkg committee bls public key file and outputs BLS12381 G1, G2 and BN254 G1 and G2 keys
///
/// # Returns
///
/// Outputs the committee threshold BLS public keys.
/// If the committee bls public key file is not present for the node, an `Aborted` error is returned.
///
pub fn print_bls_pub_key() -> Result<(), CliError> {
    let committee_key_ser = DkgCommitteeBLSPublicKeyBytes::read()?;
    let committee_key = DkgCommitteeBLSPublicKey::try_from(&committee_key_ser)?;

    println!(
        "BLS12381 Committee Threshold Public Key G1: {}",
        committee_key.bls_pub_key.bls12381.pub_key_g1
    );

    println!(
        "BLS12381 Committee Threshold Public Key G2: {}",
        committee_key.bls_pub_key.bls12381.pub_key_g2
    );

    println!(
        "BN254 Committee Threshold Public Key G1: {}",
        committee_key.bls_pub_key.bn254.pub_key_g1
    );

    println!(
        "BN254 Committee Threshold Public Key G2: {}",
        committee_key.bls_pub_key.bn254.pub_key_g2
    );
    Ok(())
}
