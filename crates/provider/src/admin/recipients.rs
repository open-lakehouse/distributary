//! The recipient handlers: recipients, their tokens and activation links.

use async_trait::async_trait;
use opensharing_admin_api::RecipientHandler;
use opensharing_admin_api::recipients::v1::{
    AuthenticationType, CreateRecipientRequest, DeleteRecipientRequest, GetRecipientRequest,
    ListRecipientsRequest, ListRecipientsResponse, Recipient, RecipientToken,
    RotateRecipientTokenRequest, UpdateRecipientRequest,
};

use super::{AdminAction, ProviderService, identifier};
use crate::Result;
use crate::recipients::{activation_url, create_recipient, rotate_token};
use crate::secrets::Secrets;
use crate::store::{RecipientRecord, RecipientUpdate, TokenRecord};
use opensharing_admin_api::{Error as AdminError, Result as AdminResult};

/// A token as shown to the provider; the activation link only until it is
/// used. An `expiration_time` of 0 means the token never expires.
pub async fn listed_token(
    secrets: &Secrets,
    public_url: &str,
    token: &TokenRecord,
) -> Result<RecipientToken> {
    Ok(RecipientToken {
        id: token.id.clone(),
        created_at: token.created_at,
        activation_url: activation_url(secrets, public_url, token)
            .await?
            .unwrap_or_default(),
        expiration_time: token.expires_at.unwrap_or_default(),
        ..Default::default()
    })
}

impl<Cx> ProviderService<Cx> {
    /// A stored recipient with its tokens.
    async fn recipient(&self, record: RecipientRecord) -> Result<Recipient> {
        let mut tokens = Vec::new();
        for token in self.store.tokens_for_recipient(&record.id).await? {
            tokens.push(listed_token(&self.secrets, &self.public_url, &token).await?);
        }
        Ok(Recipient {
            id: Some(record.id),
            name: record.name,
            authentication_type: AuthenticationType::TOKEN.into(),
            comment: record.comment,
            created_at: Some(record.created_at),
            tokens,
            ..Default::default()
        })
    }
}

#[async_trait]
impl<Cx: Send + Sync + 'static> RecipientHandler<Cx> for ProviderService<Cx> {
    type Error = AdminError;

    async fn list_recipients(
        &self,
        _request: ListRecipientsRequest,
        context: Cx,
    ) -> AdminResult<ListRecipientsResponse> {
        self.authorize(AdminAction::ListRecipients, &context)
            .await?;
        let mut recipients = Vec::new();
        for record in self.store.list_recipients().await? {
            recipients.push(self.recipient(record).await?);
        }
        Ok(ListRecipientsResponse {
            recipients,
            ..Default::default()
        })
    }

    async fn create_recipient(
        &self,
        request: CreateRecipientRequest,
        context: Cx,
    ) -> AdminResult<Recipient> {
        self.authorize(
            AdminAction::CreateRecipient {
                recipient: &request.name,
            },
            &context,
        )
        .await?;
        match request.authentication_type.as_known() {
            Some(
                AuthenticationType::TOKEN | AuthenticationType::AUTHENTICATION_TYPE_UNSPECIFIED,
            ) => {}
            _ => {
                return Err(AdminError::invalid_argument(
                    "only TOKEN authentication is supported",
                ));
            }
        }
        identifier(&request.name)?;
        let record = create_recipient(
            self.store.as_ref(),
            &self.secrets,
            &request.name,
            request.comment,
            request.expiration_time.filter(|at| *at != 0),
        )
        .await?;
        Ok(self.recipient(record).await?)
    }

    async fn get_recipient(
        &self,
        request: GetRecipientRequest,
        context: Cx,
    ) -> AdminResult<Recipient> {
        self.authorize(
            AdminAction::ReadRecipient {
                recipient: &request.name,
            },
            &context,
        )
        .await?;
        let record = self.store.get_recipient(&request.name).await?;
        Ok(self.recipient(record).await?)
    }

    /// Update a recipient's comment and its token's expiry (`expiration_time`,
    /// 0 for never). Renaming, changing the owner and setting properties are
    /// not supported.
    async fn update_recipient(
        &self,
        request: UpdateRecipientRequest,
        context: Cx,
    ) -> AdminResult<Recipient> {
        self.authorize(
            AdminAction::UpdateRecipient {
                recipient: &request.name,
            },
            &context,
        )
        .await?;
        if request.new_name.is_some() {
            return Err(AdminError::invalid_argument(
                "renaming recipients is not supported",
            ));
        }
        let update = RecipientUpdate {
            comment: request.comment,
            expires_at: request
                .expiration_time
                .map(|at| Some(at).filter(|at| *at != 0)),
        };
        let record = self.store.update_recipient(&request.name, update).await?;
        Ok(self.recipient(record).await?)
    }

    async fn delete_recipient(
        &self,
        request: DeleteRecipientRequest,
        context: Cx,
    ) -> AdminResult<()> {
        self.authorize(
            AdminAction::DeleteRecipient {
                recipient: &request.name,
            },
            &context,
        )
        .await?;
        Ok(self.store.delete_recipient(&request.name).await?)
    }

    /// Issue the recipient a fresh token. Existing tokens keep working for
    /// `existing_token_expire_in_seconds` at most; without it, or with 0, they
    /// stop at once.
    async fn rotate_recipient_token(
        &self,
        request: RotateRecipientTokenRequest,
        context: Cx,
    ) -> AdminResult<Recipient> {
        self.authorize(
            AdminAction::RotateRecipientToken {
                recipient: &request.name,
            },
            &context,
        )
        .await?;
        let record = self.store.get_recipient(&request.name).await?;
        rotate_token(
            self.store.as_ref(),
            &self.secrets,
            &record,
            request.existing_token_expire_in_seconds,
        )
        .await?;
        Ok(self.recipient(record).await?)
    }
}
