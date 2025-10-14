use bytes::Bytes;
use prost::Message;

use malachitebft_codec::Codec;
use malachitebft_core_consensus::{LivenessMsg, PeerId, ProposedValue, SignedConsensusMsg};
use malachitebft_core_types::{
    CommitCertificate, CommitSignature, NilOrVal, PolkaCertificate, PolkaSignature, Round,
    RoundCertificate, RoundCertificateType, RoundSignature, SignedVote, Validity, VoteType,
};
use malachitebft_engine::util::streaming::{StreamContent, StreamId, StreamMessage};
use malachitebft_starknet_p2p_types::{Felt, FeltExt, Signature};
use malachitebft_sync::{self as sync, ValueRequest, ValueResponse};

use crate::proto::{self as proto, Error as ProtoError, Protobuf};
use crate::types::{self as p2p, Address, BlockHash, Height, MockContext, ProposalPart, Vote};

trait MessageExt {
    fn encode_to_bytes(&self) -> Bytes;
}

impl<T> MessageExt for T
where
    T: Message,
{
    fn encode_to_bytes(&self) -> Bytes {
        Bytes::from(self.encode_to_vec())
    }
}

pub struct ProtobufCodec;

#[derive(Clone, PartialEq, ::prost::Message)]
struct ProtoHeight {
    #[prost(uint64, tag = "3")]
    pub block_number: u64,
    #[prost(uint64, tag = "4")]
    pub fork_id: u64,
}

impl Codec<Height> for ProtobufCodec {
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<Height, Self::Error> {
        let proto = ProtoHeight::decode(bytes.as_ref()).map_err(ProtoError::Decode)?;
        Ok(Height::new(proto.block_number, proto.fork_id))
    }

    fn encode(&self, height: &Height) -> Result<Bytes, Self::Error> {
        let proto = ProtoHeight {
            block_number: height.block_number,
            fork_id: height.fork_id,
        };
        Ok(proto.encode_to_bytes())
    }
}

impl Codec<Address> for ProtobufCodec {
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<Address, Self::Error> {
        Protobuf::from_bytes(&bytes)
    }

    fn encode(&self, address: &Address) -> Result<Bytes, Self::Error> {
        Protobuf::to_bytes(address)
    }
}

impl Codec<BlockHash> for ProtobufCodec {
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<BlockHash, Self::Error> {
        Protobuf::from_bytes(&bytes)
    }

    fn encode(&self, block_hash: &BlockHash) -> Result<Bytes, Self::Error> {
        Protobuf::to_bytes(block_hash)
    }
}

impl Codec<ProposalPart> for ProtobufCodec {
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<ProposalPart, Self::Error> {
        Protobuf::from_bytes(bytes.as_ref())
    }

    fn encode(&self, msg: &ProposalPart) -> Result<Bytes, Self::Error> {
        Protobuf::to_bytes(msg)
    }
}

pub fn decode_proposed_value(
    proto: proto::sync::ProposedValue,
) -> Result<ProposedValue<MockContext>, ProtoError> {
    let proposer = proto
        .proposer
        .ok_or_else(|| ProtoError::missing_field::<proto::sync::ProposedValue>("proposer"))?;

    Ok(ProposedValue {
        height: Height::new(proto.block_number, proto.fork_id),
        round: Round::from(proto.round),
        value: BlockHash::from_bytes(&proto.value)?,
        valid_round: Round::from(proto.valid_round),
        proposer: Address::from_proto(proposer)?,
        validity: Validity::from_bool(proto.validity),
    })
}

pub fn encode_proposed_value(
    msg: &ProposedValue<MockContext>,
) -> Result<proto::sync::ProposedValue, ProtoError> {
    let proto = proto::sync::ProposedValue {
        fork_id: msg.height.fork_id,
        block_number: msg.height.block_number,
        round: msg.round.as_u32().expect("round should not be nil"),
        valid_round: msg.valid_round.as_u32(),
        value: msg.value.to_bytes()?,
        proposer: Some(msg.proposer.to_proto()?),
        validity: msg.validity.to_bool(),
    };

    Ok(proto)
}

impl Codec<ProposedValue<MockContext>> for ProtobufCodec {
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<ProposedValue<MockContext>, Self::Error> {
        decode_proposed_value(proto::sync::ProposedValue::decode(bytes)?)
    }

    fn encode(&self, msg: &ProposedValue<MockContext>) -> Result<Bytes, Self::Error> {
        encode_proposed_value(msg).map(|proto| proto.encode_to_bytes())
    }
}

pub fn decode_peer_id(proto: proto::PeerId) -> Result<PeerId, ProtoError> {
    PeerId::from_bytes(&proto.id).map_err(|e| ProtoError::Other(e.to_string()))
}

pub fn encode_peer_id(peer_id: &PeerId) -> Result<proto::PeerId, ProtoError> {
    Ok(proto::PeerId {
        id: Bytes::from(peer_id.to_bytes()),
    })
}

impl Codec<PeerId> for ProtobufCodec {
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<PeerId, Self::Error> {
        decode_peer_id(proto::PeerId::decode(bytes)?)
    }

    fn encode(&self, peer_id: &PeerId) -> Result<Bytes, Self::Error> {
        encode_peer_id(peer_id).map(|proto| proto.encode_to_bytes())
    }
}

pub fn decode_sync_status(
    status: proto::sync::Status,
) -> Result<sync::Status<MockContext>, ProtoError> {
    let peer_id = status
        .peer_id
        .ok_or_else(|| ProtoError::missing_field::<proto::sync::Status>("peer_id"))?;

    Ok(sync::Status {
        peer_id: decode_peer_id(peer_id)?,
        tip_height: Height::new(status.block_number, status.fork_id),
        history_min_height: Height::new(status.earliest_block_number, status.earliest_fork_id),
    })
}

pub fn encode_sync_status(
    status: &sync::Status<MockContext>,
) -> Result<proto::sync::Status, ProtoError> {
    Ok(proto::sync::Status {
        peer_id: Some(encode_peer_id(&status.peer_id)?),
        block_number: status.tip_height.block_number,
        fork_id: status.tip_height.fork_id,
        earliest_block_number: status.history_min_height.block_number,
        earliest_fork_id: status.history_min_height.fork_id,
    })
}

impl Codec<sync::Status<MockContext>> for ProtobufCodec {
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<sync::Status<MockContext>, Self::Error> {
        decode_sync_status(proto::sync::Status::decode(bytes)?)
    }

    fn encode(&self, status: &sync::Status<MockContext>) -> Result<Bytes, Self::Error> {
        encode_sync_status(status).map(|proto| proto.encode_to_bytes())
    }
}

pub fn decode_sync_request(
    proto_request: proto::sync::SyncRequest,
) -> Result<sync::Request<MockContext>, ProtoError> {
    let messages = proto_request
        .messages
        .ok_or_else(|| ProtoError::missing_field::<proto::sync::SyncRequest>("messages"))?;
    let request = match messages {
        proto::sync::sync_request::Messages::ValueRequest(value_request) => {
            let start = Height::new(value_request.block_number, value_request.fork_id);
            let end = value_request
                .end_block_number
                .map_or(start, |end| Height::new(end, value_request.fork_id));
            sync::Request::ValueRequest(ValueRequest::new(start..=end))
        }
    };

    Ok(request)
}

pub fn encode_sync_request(
    request: &sync::Request<MockContext>,
) -> Result<proto::sync::SyncRequest, ProtoError> {
    let proto = match request {
        sync::Request::ValueRequest(value_request) => {
            let height = value_request.range.start();
            proto::sync::SyncRequest {
                messages: Some(proto::sync::sync_request::Messages::ValueRequest(
                    proto::sync::ValueRequest {
                        fork_id: height.fork_id,
                        block_number: height.block_number,
                        end_block_number: Some(value_request.range.end().block_number),
                    },
                )),
            }
        }
    };

    Ok(proto)
}

impl Codec<sync::Request<MockContext>> for ProtobufCodec {
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<sync::Request<MockContext>, Self::Error> {
        decode_sync_request(proto::sync::SyncRequest::decode(bytes)?)
    }

    fn encode(&self, request: &sync::Request<MockContext>) -> Result<Bytes, Self::Error> {
        encode_sync_request(request).map(|proto| proto.encode_to_bytes())
    }
}

pub fn decode_sync_response(
    proto_response: proto::sync::SyncResponse,
) -> Result<sync::Response<MockContext>, ProtoError> {
    let messages = proto_response
        .messages
        .ok_or_else(|| ProtoError::missing_field::<proto::sync::SyncResponse>("messages"))?;

    let response = match messages {
        proto::sync::sync_response::Messages::ValueResponse(value_response) => {
            sync::Response::ValueResponse(ValueResponse::new(
                Height::new(value_response.block_number, value_response.fork_id),
                value_response
                    .values
                    .into_iter()
                    .map(decode_synced_value)
                    .collect::<Result<Vec<_>, ProtoError>>()?,
            ))
        }
    };

    Ok(response)
}

pub fn encode_sync_response(
    response: &sync::Response<MockContext>,
) -> Result<proto::sync::SyncResponse, ProtoError> {
    let proto = match response {
        sync::Response::ValueResponse(value_response) => proto::sync::SyncResponse {
            messages: Some(proto::sync::sync_response::Messages::ValueResponse(
                proto::sync::ValueResponse {
                    fork_id: value_response.start_height.fork_id,
                    block_number: value_response.start_height.block_number,
                    values: value_response
                        .values
                        .iter()
                        .map(encode_synced_value)
                        .collect::<Result<Vec<_>, _>>()?,
                },
            )),
        },
    };

    Ok(proto)
}

impl Codec<sync::Response<MockContext>> for ProtobufCodec {
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<sync::Response<MockContext>, Self::Error> {
        decode_sync_response(proto::sync::SyncResponse::decode(bytes)?)
    }

    fn encode(&self, response: &sync::Response<MockContext>) -> Result<Bytes, Self::Error> {
        encode_sync_response(response).map(|proto| proto.encode_to_bytes())
    }
}

pub fn decode_consensus_message(
    proto: proto::Vote,
) -> Result<SignedConsensusMsg<MockContext>, ProtoError> {
    let vote = Vote::from_proto(proto)?;
    let signature = p2p::Signature::test();

    Ok(SignedConsensusMsg::Vote(SignedVote::new(vote, signature)))
}

pub fn encode_consensus_message(
    msg: &SignedConsensusMsg<MockContext>,
) -> Result<proto::Vote, ProtoError> {
    let message = match msg {
        SignedConsensusMsg::Vote(v) => v.to_proto()?,
        SignedConsensusMsg::Proposal(_) => {
            panic!("explicit proposal not supported by starknet test application")
        } // SignedConsensusMsg::Proposal(p) => proto::ConsensusMessage {
          //     messages: Some(Messages::Proposal(p.to_proto()?)),
          //     signature: Some(p.signature.to_proto()?),
          // },
    };

    Ok(message)
}

impl Codec<SignedConsensusMsg<MockContext>> for ProtobufCodec {
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<SignedConsensusMsg<MockContext>, Self::Error> {
        decode_consensus_message(proto::Vote::decode(bytes)?)
    }

    fn encode(&self, msg: &SignedConsensusMsg<MockContext>) -> Result<Bytes, Self::Error> {
        encode_consensus_message(msg).map(|proto| proto.encode_to_bytes())
    }
}

pub(crate) fn encode_round_certificate(
    certificate: &RoundCertificate<MockContext>,
) -> Result<proto::RoundCertificate, ProtoError> {
    Ok(proto::RoundCertificate {
        fork_id: certificate.height.fork_id,
        block_number: certificate.height.block_number,
        round: certificate.round.as_u32().expect("round should not be nil"),
        cert_type: match certificate.cert_type {
            RoundCertificateType::Precommit => {
                proto::RoundCertificateType::RoundCertPrecommit.into()
            }
            RoundCertificateType::Skip => proto::RoundCertificateType::RoundCertSkip.into(),
        },
        signatures: certificate
            .round_signatures
            .iter()
            .map(|sig| -> Result<proto::RoundSignature, ProtoError> {
                let address = sig.address.to_proto()?;
                let signature = encode_signature(&sig.signature)?;
                let block_hash = match sig.value_id {
                    NilOrVal::Nil => None,
                    NilOrVal::Val(value_id) => Some(value_id.to_proto()?),
                };
                Ok(proto::RoundSignature {
                    vote_type: match sig.vote_type {
                        VoteType::Prevote => proto::VoteType::Prevote.into(),
                        VoteType::Precommit => proto::VoteType::Precommit.into(),
                    },
                    validator_address: Some(address),
                    signature: Some(signature),
                    block_hash,
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

pub(crate) fn decode_round_certificate(
    certificate: proto::RoundCertificate,
) -> Result<RoundCertificate<MockContext>, ProtoError> {
    Ok(RoundCertificate {
        height: Height::new(certificate.block_number, certificate.fork_id),
        round: Round::new(certificate.round),
        cert_type: match proto::RoundCertificateType::try_from(certificate.cert_type)
            .map_err(|_| ProtoError::Other("Unknown RoundCertificateType".into()))?
        {
            proto::RoundCertificateType::RoundCertPrecommit => RoundCertificateType::Precommit,
            proto::RoundCertificateType::RoundCertSkip => RoundCertificateType::Skip,
        },
        round_signatures: certificate
            .signatures
            .into_iter()
            .map(|sig| -> Result<RoundSignature<MockContext>, ProtoError> {
                let address = sig.validator_address.ok_or_else(|| {
                    ProtoError::missing_field::<proto::RoundCertificate>("validator_address")
                })?;
                let signature = sig.signature.ok_or_else(|| {
                    ProtoError::missing_field::<proto::RoundCertificate>("signature")
                })?;
                let signature = decode_signature(signature)?;
                let address = Address::from_proto(address)?;
                let value_id = match sig.block_hash {
                    None => NilOrVal::Nil,
                    Some(block_hash) => NilOrVal::Val(BlockHash::from_proto(block_hash)?),
                };
                let vote_type = match proto::VoteType::try_from(sig.vote_type)
                    .map_err(|_| ProtoError::Other("Invalid vote type".to_string()))?
                {
                    proto::VoteType::Prevote => VoteType::Prevote,
                    proto::VoteType::Precommit => VoteType::Precommit,
                };
                Ok(RoundSignature::new(vote_type, value_id, address, signature))
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

impl Codec<LivenessMsg<MockContext>> for ProtobufCodec {
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<LivenessMsg<MockContext>, Self::Error> {
        let msg = proto::LivenessMessage::decode(bytes.as_ref())?;
        match msg.message {
            Some(proto::liveness_message::Message::Vote(vote)) => {
                Ok(LivenessMsg::Vote(decode_vote(vote)?))
            }
            Some(proto::liveness_message::Message::PolkaCertificate(cert)) => Ok(
                LivenessMsg::PolkaCertificate(decode_polka_certificate(cert)?),
            ),
            Some(proto::liveness_message::Message::RoundCertificate(cert)) => Ok(
                LivenessMsg::SkipRoundCertificate(decode_round_certificate(cert)?),
            ),
            None => Err(ProtoError::missing_field::<proto::LivenessMessage>(
                "message",
            )),
        }
    }

    fn encode(&self, msg: &LivenessMsg<MockContext>) -> Result<Bytes, Self::Error> {
        match msg {
            LivenessMsg::Vote(vote) => {
                let message = encode_vote(vote)?;
                Ok(Bytes::from(
                    proto::LivenessMessage {
                        message: Some(proto::liveness_message::Message::Vote(message)),
                    }
                    .encode_to_vec(),
                ))
            }
            LivenessMsg::PolkaCertificate(cert) => {
                let message = encode_polka_certificate(cert)?;
                Ok(Bytes::from(
                    proto::LivenessMessage {
                        message: Some(proto::liveness_message::Message::PolkaCertificate(message)),
                    }
                    .encode_to_vec(),
                ))
            }
            LivenessMsg::SkipRoundCertificate(cert) => {
                let message = encode_round_certificate(cert)?;
                Ok(Bytes::from(
                    proto::LivenessMessage {
                        message: Some(proto::liveness_message::Message::RoundCertificate(message)),
                    }
                    .encode_to_vec(),
                ))
            }
        }
    }
}

impl<T> Codec<StreamMessage<T>> for ProtobufCodec
where
    T: Protobuf,
{
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<StreamMessage<T>, Self::Error> {
        let p2p_msg = p2p::StreamMessage::from_bytes(&bytes)?;

        Ok(StreamMessage {
            stream_id: StreamId::new(p2p_msg.id),
            sequence: p2p_msg.sequence,
            content: match p2p_msg.content {
                p2p::StreamContent::Data(data) => {
                    StreamContent::Data(T::from_bytes(data.as_ref())?)
                }
                p2p::StreamContent::Fin => StreamContent::Fin,
            },
        })
    }

    fn encode(&self, msg: &StreamMessage<T>) -> Result<Bytes, Self::Error> {
        let p2p_msg = p2p::StreamMessage {
            id: msg.stream_id.to_bytes(),
            sequence: msg.sequence,
            content: match &msg.content {
                StreamContent::Data(data) => p2p::StreamContent::Data(data.to_bytes()?),
                StreamContent::Fin => p2p::StreamContent::Fin,
            },
        };

        p2p_msg.to_bytes()
    }
}

pub fn encode_signature(_signature: &Signature) -> Result<proto::ConsensusSignature, ProtoError> {
    Ok(proto::ConsensusSignature {
        r: Some(Felt::ONE.to_proto()?),
        s: Some(Felt::ONE.to_proto()?),
    })
}

pub fn decode_signature(_signature: proto::ConsensusSignature) -> Result<Signature, ProtoError> {
    Ok(Signature::test())
}

pub fn decode_commit_certificate(
    certificate: proto::sync::CommitCertificate,
) -> Result<CommitCertificate<MockContext>, ProtoError> {
    let value_id = if let Some(block_hash) = certificate.block_hash {
        BlockHash::from_proto(block_hash)?
    } else {
        return Err(ProtoError::missing_field::<proto::sync::CommitCertificate>(
            "block_hash",
        ));
    };

    let commit_signatures = certificate
        .signatures
        .into_iter()
        .map(|sig| -> Result<CommitSignature<MockContext>, ProtoError> {
            let address = sig.validator_address.ok_or_else(|| {
                ProtoError::missing_field::<proto::sync::CommitCertificate>("validator_address")
            })?;
            let signature = sig.signature.ok_or_else(|| {
                ProtoError::missing_field::<proto::sync::CommitCertificate>("signature")
            })?;
            let signature = decode_signature(signature)?;
            let address = Address::from_proto(address)?;
            Ok(CommitSignature::new(address, signature))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let certificate = CommitCertificate {
        height: Height::new(certificate.block_number, certificate.fork_id),
        round: Round::new(certificate.round),
        value_id,
        commit_signatures,
    };

    Ok(certificate)
}

pub fn encode_commit_certificate(
    certificate: &CommitCertificate<MockContext>,
) -> Result<proto::sync::CommitCertificate, ProtoError> {
    Ok(proto::sync::CommitCertificate {
        fork_id: certificate.height.fork_id,
        block_number: certificate.height.block_number,
        round: certificate.round.as_u32().expect("round should not be nil"),
        block_hash: Some(certificate.value_id.to_proto()?),
        signatures: certificate
            .commit_signatures
            .iter()
            .map(|sig| -> Result<proto::sync::CommitSignature, ProtoError> {
                let address = sig.address.to_proto()?;
                let signature = encode_signature(&sig.signature)?;
                Ok(proto::sync::CommitSignature {
                    validator_address: Some(address),
                    signature: Some(signature),
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

impl Codec<CommitCertificate<MockContext>> for ProtobufCodec {
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<CommitCertificate<MockContext>, Self::Error> {
        decode_commit_certificate(
            proto::sync::CommitCertificate::decode(bytes).map_err(ProtoError::Decode)?,
        )
    }

    fn encode(&self, msg: &CommitCertificate<MockContext>) -> Result<Bytes, Self::Error> {
        encode_commit_certificate(msg).map(|proto| proto.encode_to_bytes())
    }
}

// NOTE: Will be used again in #997
#[allow(dead_code)]
pub(crate) fn encode_polka_certificate(
    certificate: &PolkaCertificate<MockContext>,
) -> Result<proto::PolkaCertificate, ProtoError> {
    Ok(proto::PolkaCertificate {
        fork_id: certificate.height.fork_id,
        block_number: certificate.height.block_number,
        block_hash: Some(certificate.value_id.to_proto()?),
        round: certificate.round.as_u32().unwrap(),
        signatures: certificate
            .polka_signatures
            .iter()
            .map(|sig| -> Result<proto::PolkaSignature, ProtoError> {
                let address = sig.address.to_proto()?;
                let signature = encode_signature(&sig.signature)?;
                Ok(proto::PolkaSignature {
                    validator_address: Some(address),
                    signature: Some(signature),
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

// NOTE: Will be used again in #997
#[allow(dead_code)]
pub(crate) fn decode_polka_certificate(
    certificate: proto::PolkaCertificate,
) -> Result<PolkaCertificate<MockContext>, ProtoError> {
    let block_hash = certificate
        .block_hash
        .ok_or_else(|| ProtoError::missing_field::<proto::PolkaCertificate>("block_hash"))?;

    Ok(PolkaCertificate {
        height: Height::new(certificate.block_number, certificate.fork_id),
        round: Round::new(certificate.round),
        value_id: BlockHash::from_proto(block_hash)?,
        polka_signatures: certificate
            .signatures
            .into_iter()
            .map(|sig| -> Result<PolkaSignature<MockContext>, ProtoError> {
                let address = sig.validator_address.ok_or_else(|| {
                    ProtoError::missing_field::<proto::PolkaCertificate>("validator_address")
                })?;
                let signature = sig.signature.ok_or_else(|| {
                    ProtoError::missing_field::<proto::PolkaCertificate>("signature")
                })?;
                let signature = decode_signature(signature)?;
                let address = Address::from_proto(address)?;
                Ok(PolkaSignature::new(address, signature))
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

impl Codec<PolkaCertificate<MockContext>> for ProtobufCodec {
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<PolkaCertificate<MockContext>, Self::Error> {
        decode_polka_certificate(proto::PolkaCertificate::decode(bytes)?)
    }

    fn encode(&self, msg: &PolkaCertificate<MockContext>) -> Result<Bytes, Self::Error> {
        encode_polka_certificate(msg).map(|proto| proto.encode_to_bytes())
    }
}

pub fn encode_synced_value(
    synced_value: &sync::RawDecidedValue<MockContext>,
) -> Result<proto::sync::SyncedValue, ProtoError> {
    Ok(proto::sync::SyncedValue {
        value_bytes: synced_value.value_bytes.clone(),
        certificate: Some(encode_commit_certificate(&synced_value.certificate)?),
    })
}

pub fn decode_synced_value(
    proto: proto::sync::SyncedValue,
) -> Result<sync::RawDecidedValue<MockContext>, ProtoError> {
    let Some(certificate) = proto.certificate else {
        return Err(ProtoError::missing_field::<proto::sync::SyncedValue>(
            "certificate",
        ));
    };

    Ok(sync::RawDecidedValue {
        value_bytes: proto.value_bytes,
        certificate: decode_commit_certificate(certificate)?,
    })
}

impl Codec<sync::RawDecidedValue<MockContext>> for ProtobufCodec {
    type Error = ProtoError;

    fn decode(&self, bytes: Bytes) -> Result<sync::RawDecidedValue<MockContext>, Self::Error> {
        let proto = proto::sync::SyncedValue::decode(bytes).map_err(ProtoError::Decode)?;
        decode_synced_value(proto)
    }

    fn encode(&self, msg: &sync::RawDecidedValue<MockContext>) -> Result<Bytes, Self::Error> {
        Ok(Bytes::from(encode_synced_value(msg)?.encode_to_vec()))
    }
}

// NOTE: Will be used again in #997
#[allow(dead_code)]
pub(crate) fn encode_vote(vote: &SignedVote<MockContext>) -> Result<proto::Vote, ProtoError> {
    vote.message.to_proto()
}

// NOTE: Will be used again in #997
#[allow(dead_code)]
pub(crate) fn decode_vote(msg: proto::Vote) -> Result<SignedVote<MockContext>, ProtoError> {
    let signature = Signature::test();
    let vote = Vote::from_proto(msg)?;
    Ok(SignedVote::new(vote, signature))
}
