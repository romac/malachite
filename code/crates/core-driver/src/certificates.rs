//! Retain signed certificates and construct the proofs used to enter a round.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use malachitebft_core_types::{
    Context, EnterRoundCertificate, ExtendedCommitCertificate, NilOrVal, PolkaCertificate,
    PolkaSignature, Round, RoundCertificate, RoundCertificateType, RoundSignature, SignedVote,
    Threshold, ValueId, Vote, VoteType,
};

use crate::Driver;

impl<Ctx: Context> Driver<Ctx> {
    /// Import the signed votes for equivocation accounting and vote extensions.
    /// A certificate is one driver input. Intermediate vote-keeper notifications
    /// while importing its signatures do not trigger separate round transitions.
    pub(super) fn record_commit_certificate(
        &mut self,
        certificate: ExtendedCommitCertificate<Ctx>,
    ) {
        let local_round = self.round();
        for signature in &certificate.commit_signatures {
            let mut vote = self.ctx.new_precommit(
                certificate.height,
                certificate.round,
                NilOrVal::Val(certificate.value_id.clone()),
                signature.address.clone(),
            );
            if let Some(extension) = signature.extension.clone() {
                vote = vote.extend(extension);
            }
            self.vote_keeper.apply_vote(
                SignedVote::new(vote, signature.signature.clone()),
                local_round,
            );
        }
        self.commit_certificates.push(certificate);
    }

    pub(super) fn record_polka_certificate(&mut self, certificate: PolkaCertificate<Ctx>) {
        let local_round = self.round();
        for vote in certificate.votes(&self.ctx) {
            self.vote_keeper.apply_vote(vote, local_round);
        }
        // A second certificate may supply previously unseen equivocation evidence.
        if self
            .polka_certificate(certificate.round, &certificate.value_id)
            .is_none()
        {
            self.polka_certificates.push(certificate);
        }
    }

    pub(super) fn build_polka_certificate(&mut self, vote_round: Round, value_id: &ValueId<Ctx>) {
        let Some(per_round) = self.vote_keeper.per_round(vote_round) else {
            return;
        };

        self.polka_certificates.push(PolkaCertificate {
            height: self.height(),
            round: vote_round,
            value_id: value_id.clone(),
            polka_signatures: per_round
                .received_votes()
                .iter()
                .filter(|v| {
                    v.vote_type() == VoteType::Prevote
                        && v.value().as_ref() == NilOrVal::Val(value_id)
                })
                .map(|v| PolkaSignature::new(v.validator_address().clone(), v.signature.clone()))
                .collect(),
        })
    }

    /// Prunes all polka certificates and votes from rounds less than `min_round`.
    pub fn prune_votes_and_certificates(&mut self, min_round: Round) {
        self.prune_polka_certificates(min_round);
        self.vote_keeper.prune_votes(min_round);
    }

    /// Prunes all polka certificates from rounds less than `min_round`.
    fn prune_polka_certificates(&mut self, min_round: Round) {
        self.polka_certificates
            .retain(|cert| cert.round >= min_round);
    }

    pub(super) fn store_precommit_any_round_certificate(&mut self, vote_round: Round) {
        let Some(per_round) = self.vote_keeper.per_round(vote_round) else {
            panic!("Missing the PrecommitAny votes for round {vote_round}");
        };

        let precommits: Vec<SignedVote<Ctx>> = per_round
            .received_votes()
            .iter()
            .filter(|v| v.vote_type() == VoteType::Precommit)
            .cloned()
            .collect();

        self.round_certificate = Some(EnterRoundCertificate::new_from_votes(
            self.height(),
            vote_round.increment(),
            vote_round,
            RoundCertificateType::Precommit,
            precommits,
        ));
    }

    pub(super) fn store_skip_round_certificate(&mut self, vote_round: Round) {
        let Some(per_round) = self.vote_keeper.per_round(vote_round) else {
            panic!("Missing the SkipRoundvotes for round {vote_round}");
        };

        // NOTE: We include all received votes even though only f+1 are needed
        // for a Skip round certificate. Could be trimmed if needed.
        let mut seen_addresses = BTreeSet::new();
        let skip_votes: Vec<_> = per_round
            .received_votes()
            .iter()
            .filter(|vote| seen_addresses.insert(vote.validator_address()))
            .cloned()
            .collect();

        self.round_certificate = Some(EnterRoundCertificate::new_from_votes(
            self.height(),
            vote_round,
            vote_round,
            RoundCertificateType::Skip,
            skip_votes,
        ));
    }

    pub(super) fn store_round_certificate_from_commit_certificate(
        &mut self,
        certificate: &ExtendedCommitCertificate<Ctx>,
    ) {
        // NOTE: We include all 2f+1 signatures from the commit certificate even though
        // only f+1 are needed for a Skip round certificate. Could be trimmed if needed.
        let commit_certificate = certificate.trim_vote_extensions();
        self.round_certificate = Some(EnterRoundCertificate::from_commit_certificate(
            &commit_certificate,
            RoundCertificateType::Skip,
            certificate.round,
        ));
    }

    pub(super) fn store_round_certificate_from_polka_certificate(
        &mut self,
        certificate: &PolkaCertificate<Ctx>,
    ) {
        let round_signatures = certificate
            .polka_signatures
            .iter()
            .map(|ps| {
                RoundSignature::new(
                    VoteType::Prevote,
                    NilOrVal::Val(certificate.value_id.clone()),
                    ps.address.clone(),
                    ps.signature.clone(),
                )
            })
            .collect();

        self.round_certificate = Some(EnterRoundCertificate {
            certificate: RoundCertificate {
                height: certificate.height,
                round: certificate.round,
                cert_type: RoundCertificateType::Skip,
                round_signatures,
            },
            enter_round: certificate.round,
        });
    }

    /// Ensure a precommit round certificate is stored for the given round.
    ///
    /// Called when TimeoutPrecommit fires and no round certificate exists yet.
    /// Looks for evidence of 2f+1 precommits in:
    ///  1. The vote keeper (individual precommit votes), or
    ///  2. A stored commit certificate for the round.
    pub(super) fn ensure_precommit_round_certificate(&mut self, round: Round) {
        // Try the vote keeper first: if we have 2f+1 precommits from individual votes
        if self
            .vote_keeper
            .is_threshold_met(&round, VoteType::Precommit, Threshold::Any)
        {
            self.store_precommit_any_round_certificate(round);
            return;
        }

        // Otherwise, try a stored commit certificate for this round
        if let Some(cert) = self
            .commit_certificates
            .iter()
            .find(|c| c.round == round && c.height == self.height())
        {
            let commit_certificate = cert.trim_vote_extensions();
            self.round_certificate = Some(EnterRoundCertificate::from_commit_certificate(
                &commit_certificate,
                RoundCertificateType::Precommit,
                cert.round.increment(),
            ));
        }
    }
}
