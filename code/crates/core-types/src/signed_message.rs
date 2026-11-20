use core::ops::Deref;

/// A signed message, ie. a message emitted by a validator and signed by its private key.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SignedMessage<Msg, Sig> {
    /// The message
    pub message: Msg,

    /// The signature of the proposal.
    pub signature: Sig,
}

impl<Msg, Sig> SignedMessage<Msg, Sig> {
    /// Create a new signed message from the given message and signature.
    pub fn new(message: Msg, signature: Sig) -> Self {
        Self { message, signature }
    }

    /// Map the message to a new message.
    pub fn map<F, NewMsg>(self, f: F) -> SignedMessage<NewMsg, Sig>
    where
        F: FnOnce(Msg) -> NewMsg,
    {
        SignedMessage {
            message: f(self.message),
            signature: self.signature,
        }
    }

    /// Return a reference to the signed message.
    pub fn as_ref(&self) -> SignedMessage<&Msg, Sig>
    where
        Sig: Clone,
    {
        SignedMessage {
            message: &self.message,
            signature: self.signature.clone(),
        }
    }
}

impl<Msg, Sig> Deref for SignedMessage<Msg, Sig> {
    type Target = Msg;

    fn deref(&self) -> &Self::Target {
        &self.message
    }
}
