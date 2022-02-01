use orchard;
use pasta_curves::pallas;

use pyo3::prelude::*;
use pyo3::wrap_pyfunction;
use rand_chacha;
use rand_core::SeedableRng;
use std::convert::TryFrom;

use pasta_curves::group::ff::PrimeField;

use zcash_primitives::transaction::components::{amount::Amount, orchard::write_v5_bundle};

#[pymodule]
fn pyorchard(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Random>()?;
    m.add_class::<Address>()?;
    m.add_class::<FullViewingKey>()?;
    m.add_class::<Note>()?;
    m.add_class::<Spend>()?;
    m.add_class::<Output>()?;
    m.add_class::<Builder>()?;
    m.add_class::<Bundle>()?;
    m.add_class::<ProvingKey>()?;
    m.add_wrapped(wrap_pyfunction!(experiment))?;
    //m.add_wrapped(wrap_pyfunction!(idiv))?;
    Ok(())
}

//create_exception!(procmaps, ParseError, PyException);

#[pyclass]
#[derive(Clone, Debug)]
struct Note {
    inner: orchard::Note,
}

#[pymethods]
impl Note {
    #[staticmethod]
    fn from_bytes(bytes: [u8; 115]) -> PyResult<Note> {
        Ok(Note {
            inner: orchard::hww::deserialize_note(bytes).expect("note not deserializable"),
        })
    }

    fn to_bytes(&self) -> [u8; 115] {
        orchard::hww::serialize_note(self.inner)
    }
}

#[pyclass]
#[derive(Clone, Debug)]
struct FullViewingKey {
    inner: orchard::keys::FullViewingKey,
}

#[pymethods]
impl FullViewingKey {
    #[staticmethod]
    fn from_bytes(bytes: [u8; 96]) -> PyResult<FullViewingKey> {
        Ok(FullViewingKey {
            inner: orchard::keys::FullViewingKey::from_bytes(&bytes)
                .expect("fvk not deserializable"),
        })
    }
}

#[pyclass]
#[derive(Clone, Debug)]
struct Random(rand_chacha::ChaCha12Rng);

#[pymethods]
impl Random {
    #[staticmethod]
    fn from_seed(seed: [u8; 32]) -> Random {
        Random(rand_chacha::ChaCha12Rng::from_seed(seed))
    }

    /// testing
    #[staticmethod]
    fn default() -> Self {
        Random::from_seed([0u8; 32])
    }
}

#[pyclass]
#[derive(Clone)]
struct Spend {
    fvk: FullViewingKey,
    note: Note,
}

#[pymethods]
impl Spend {
    #[new]
    fn new(fvk: FullViewingKey, note: Note) -> Self {
        Spend { fvk, note }
    }
}

#[pyclass]
#[derive(Clone)]
struct Output {
    ovk: Option<[u8; 32]>,
    recipient: Address,
    value: u64,
    memo: Option<[u8; 512]>,
}

#[pymethods]
impl Output {
    #[new]
    fn new(ovk: Option<[u8; 32]>, recipient: Address, value: u64, memo: Option<[u8; 512]>) -> Self {
        Output {
            ovk,
            recipient,
            value,
            memo,
        }
    }

    /// testing
    #[staticmethod]
    fn default() -> Self {
        Output {
            ovk: None,
            recipient: Address::default(),
            value: 1_00_000_000,
            memo: None,
        }
    }
}

#[pyclass]
#[derive(Clone)]
struct Address(orchard::Address);

#[pymethods]
impl Address {
    #[staticmethod]
    fn from_bytes(bytes: [u8; 43]) -> PyResult<Self> {
        Ok(Address(
            Option::from(orchard::Address::from_raw_address_bytes(&bytes))
                .expect("address not deserializable"),
        ))
    }

    #[staticmethod]
    fn default() -> Self {
        Self::from_bytes([
            143, 243, 56, 105, 113, 203, 100, 184, 231, 120, 153, 8, 221, 142, 189, 125, 233, 42,
            104, 229, 134, 163, 77, 184, 254, 169, 153, 239, 210, 1, 111, 174, 118, 117, 10, 250,
            231, 238, 148, 22, 70, 188, 185,
        ])
        .unwrap()
    }
}

#[pyclass]
struct Builder(Option<orchard::builder::Builder>);

#[pymethods]
impl Builder {
    #[new]
    #[args(spends_enabled = "true", outputs_enabled = "true")]
    fn new(anchor: [u8; 32], spends_enabled: bool, outputs_enabled: bool) -> Self {
        let flags = orchard::bundle::Flags::from_parts(spends_enabled, outputs_enabled);
        let anchor = orchard::Anchor::from_bytes(anchor);
        let anchor = Option::from(anchor).expect("anchor not deserializable");
        Builder(Some(orchard::builder::Builder::new(flags, anchor)))
    }

    /// testing
    #[staticmethod]
    fn default() -> Self {
        Builder::new([1u8; 32], true, true)
    }

    fn is_some(&self) -> bool {
        self.0.is_some()
    }

    fn build(&mut self, rng: &mut Random) -> PyResult<Bundle> {
        assert!(self.is_some());
        Ok(Bundle(Authorization::UnprovenAndUnauthorized(Some(
            std::mem::take(&mut self.0)
                .unwrap()
                .build(&mut rng.0)
                .expect("cannot build"),
        ))))
    }

    fn add_spend(&mut self, spend: Spend) -> PyResult<()> {
        assert!(self.is_some());
        let mut rng = rand_chacha::ChaCha12Rng::from_seed([0u8; 32]);
        let merkle_path = orchard::hww::get_dummy_merkle_path(&mut rng);
        self.0
            .as_mut()
            .unwrap()
            .add_spend(spend.fvk.inner.clone(), spend.note.inner, merkle_path)
            .expect("cannot add spend");
        Ok(())
    }

    fn add_output(&mut self, output: Output) -> PyResult<()> {
        assert!(self.is_some());
        let ovk = match output.ovk {
            None => None,
            Some(bytes) => Some(orchard::keys::OutgoingViewingKey::from(
                <[u8; 32]>::try_from(bytes)?,
            )),
        };
        let recipient = output.recipient.0;
        let value = orchard::value::NoteValue::from_raw(output.value);
        let memo = match output.memo {
            None => None,
            Some(bytes) => Some(bytes),
        };
        self.0
            .as_mut()
            .unwrap()
            .add_recipient(ovk, recipient, value, memo)
            .expect("cannot add recipient");
        Ok(())
    }
}

#[pyclass]
struct Builder2(Option<orchard::builder::Builder>);

use orchard::{
    builder::{InProgress, InProgressSignatures, PartiallyAuthorized, Unauthorized, Unproven},
    bundle::Authorized,
    circuit::Proof,
};

fn step_create_proof<S: InProgressSignatures>(
    bundle: &mut Option<orchard::Bundle<InProgress<Unproven, S>, Amount>>,
    pk: &ProvingKey,
    rng: &mut Random,
) -> Option<orchard::Bundle<InProgress<Proof, S>, Amount>> {
    Some(
        std::mem::take(bundle)
            .unwrap()
            .create_proof(&pk.0, &mut rng.0)
            .expect("proving failed"),
    )
}

fn step_prepare<P>(
    bundle: &mut Option<orchard::Bundle<InProgress<P, Unauthorized>, Amount>>,
    rng: &mut Random,
    sighash: [u8; 32],
) -> Option<orchard::Bundle<InProgress<P, PartiallyAuthorized>, Amount>> {
    Some(std::mem::take(bundle).unwrap().prepare(&mut rng.0, sighash))
}

use orchard::primitives::redpallas;
fn step_append_signature<P>(
    bundle: &mut Option<orchard::Bundle<InProgress<P, PartiallyAuthorized>, Amount>>,
    expected_alpha: [u8; 32],
    signature: [u8; 64],
    rng: &mut Random,
) -> Option<orchard::Bundle<InProgress<P, PartiallyAuthorized>, Amount>> {
    let alpha = pallas::Scalar::from_repr(expected_alpha);
    let alpha = Option::from(alpha).expect("cannot unwrap alpha");
    let signature = redpallas::Signature::<redpallas::SpendAuth>::from(signature);
    Some(
        std::mem::take(bundle)
            .unwrap()
            .append_signature(alpha, &signature, &mut rng.0)
            .expect("cannot append a signature"),
    )
}

fn step_finalize(
    bundle: &mut Option<orchard::Bundle<InProgress<Proof, PartiallyAuthorized>, Amount>>,
) -> Option<orchard::Bundle<Authorized, Amount>> {
    Some(
        std::mem::take(bundle)
            .unwrap()
            .finalize()
            .expect("cannot finalize"),
    )
}

/*
build        :: ()                                                -> Bundle<InProgress<Unproven, Unauthorized>, V>
create_proof :: Bundle<InProgress<Unproven, S>, V>                -> Bundle<InProgress<Proof, S>, V>
prepare      :: Bundle<InProgress<P, Unauthorized>, V>            -> Bundle<InProgress<P, PartiallyAuthorized>, V>
sign         :: Bundle<InProgress<P, PartiallyAuthorized>, V>     -> Bundle<InProgress<P, PartiallyAuthorized>, V>
finalize     :: Bundle<InProgress<Proof, PartiallyAuthorized>, V> -> Bundle<Authorized, V>

apply_signatures :: Bundle<InProgress<Proof, Unauthorized>, V> -> Bundle<Authorized, V>

*/

/*impl From<orchard::builder::Error> for PyErr {
    fn from(error: orchard::builder::Error) {
        orchard::builder::Error::MissingSignatures =>
        orchard::builder::Error::Proof(_) =>
        orchard::builder::Error::ValueSum(_) => PyErr::
    }
}*/

enum Authorization {
    UnprovenAndUnauthorized(Option<orchard::Bundle<InProgress<Unproven, Unauthorized>, Amount>>),
    UnprovenAndPartiallyAuthorized(
        Option<orchard::Bundle<InProgress<Unproven, PartiallyAuthorized>, Amount>>,
    ),
    ProofAndUnauthorized(Option<orchard::Bundle<InProgress<Proof, Unauthorized>, Amount>>),
    ProofAndPartiallyAuthorized(
        Option<orchard::Bundle<InProgress<Proof, PartiallyAuthorized>, Amount>>,
    ),
    Authorized(Option<orchard::Bundle<orchard::bundle::Authorized, Amount>>),
}

#[pyclass]
struct Bundle(Authorization);

// private methods
impl Bundle {
    fn is_some(&self) -> bool {
        match &self.0 {
            Authorization::UnprovenAndUnauthorized(o) => o.is_some(),
            Authorization::UnprovenAndPartiallyAuthorized(o) => o.is_some(),
            Authorization::ProofAndUnauthorized(o) => o.is_some(),
            Authorization::ProofAndPartiallyAuthorized(o) => o.is_some(),
            Authorization::Authorized(o) => o.is_some(),
        }
    }
}

#[pymethods]
impl Bundle {
    /// The state of the `Bundle`.
    fn state(&self) -> &str {
        if self.is_some() {
            "Broken"
        } else {
            match &self.0 {
                Authorization::UnprovenAndUnauthorized(_) => "Unproven & Unauthorized",
                Authorization::UnprovenAndPartiallyAuthorized(_) => {
                    "Unproven & PartiallyAuthorized"
                }
                Authorization::ProofAndUnauthorized(_) => "Proven & Unauthorized",
                Authorization::ProofAndPartiallyAuthorized(_) => "Proven & PartiallyAuthorized",
                Authorization::Authorized(_) => "Authorized",
            }
        }
    }

    fn create_proof(&mut self, pk: &ProvingKey, rng: &mut Random) -> PyResult<()> {
        assert!(self.is_some());
        self.0 = match &mut self.0 {
            Authorization::UnprovenAndUnauthorized(b) => {
                Authorization::ProofAndUnauthorized(step_create_proof(b, pk, rng))
            }
            Authorization::UnprovenAndPartiallyAuthorized(b) => {
                Authorization::ProofAndPartiallyAuthorized(step_create_proof(b, pk, rng))
            }
            _ => panic!("cannot create a proof at this state"),
        };
        Ok(())
    }

    fn prepare(&mut self, rng: &mut Random, sighash: [u8; 32]) -> PyResult<()> {
        assert!(self.is_some());
        self.0 = match &mut self.0 {
            Authorization::UnprovenAndUnauthorized(b) => {
                Authorization::UnprovenAndPartiallyAuthorized(step_prepare(b, rng, sighash))
            }
            Authorization::ProofAndUnauthorized(b) => {
                Authorization::ProofAndPartiallyAuthorized(step_prepare(b, rng, sighash))
            }
            _ => panic!("cannot prepare at this state"),
        };
        Ok(())
    }

    fn append_signature(
        &mut self,
        expected_alpha: [u8; 32],
        signature: [u8; 64],
        rng: &mut Random,
    ) -> PyResult<()> {
        assert!(self.is_some());
        self.0 = match &mut self.0 {
            Authorization::UnprovenAndPartiallyAuthorized(b) => {
                Authorization::UnprovenAndPartiallyAuthorized(step_append_signature(
                    b,
                    expected_alpha,
                    signature,
                    rng,
                ))
            }
            Authorization::ProofAndPartiallyAuthorized(b) => {
                Authorization::ProofAndPartiallyAuthorized(step_append_signature(
                    b,
                    expected_alpha,
                    signature,
                    rng,
                ))
            }
            _ => panic!("cannot append a signature at this state"),
        };
        Ok(())
    }

    fn finalize(&mut self) -> PyResult<()> {
        assert!(self.is_some());
        self.0 = match &mut self.0 {
            Authorization::ProofAndPartiallyAuthorized(b) => {
                Authorization::Authorized(step_finalize(b))
            }
            _ => panic!("cannot finalize at this state"),
        };
        Ok(())
    }

    fn serialized(&self) -> PyResult<Vec<u8>> {
        let mut serialized = Vec::<u8>::new();
        match &self.0 {
            Authorization::Authorized(b) => {
                write_v5_bundle(b.as_ref(), &mut serialized).expect("cannot serialize")
            }
            _ => panic!("cannot serialize at this state"),
        };
        Ok(serialized)
    }
}

#[pyclass]
struct ProvingKey(orchard::circuit::ProvingKey);

#[pymethods]
impl ProvingKey {
    #[staticmethod]
    fn build() -> Self {
        ProvingKey(orchard::circuit::ProvingKey::build())
    }
}

#[pyfunction]
fn experiment() -> PyResult<Vec<u8>> {
    let mut b = Builder::default();
    let o = Output::default();
    b.add_output(o)?;
    let mut rng = Random::default();
    let sh = [0u8; 32];
    let mut b = b.build(&mut rng)?;
    let pk = ProvingKey::build();
    b.prepare(&mut rng, sh)?;
    b.create_proof(&pk, &mut rng)?;
    b.finalize()?;
    Ok(b.serialized()?)
}
