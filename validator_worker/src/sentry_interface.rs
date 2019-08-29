use primitives::{Channel, ValidatorDesc, };
use primitives::sentry::{
    ChannelAllResponse, 
    SuccessResponse,
    ValidatorMessageResponse,
    LastApprovedResponse,
    EventAggregateResponse
};
use primitives::validator::{MessageTypes};
use primitives::adapter::{Adapter};
use futures::compat::Future01CompatExt;
use futures::future::{ok, try_join_all, FutureExt, TryFutureExt};
use futures::Future;
use futures_legacy::Future as LegacyFuture;
use reqwest::r#async::{Client, Response};
use reqwest::Error;
use serde::Deserialize;
use std::iter::once;
use std::time::Duration;
use crate::error::{ValidatorWorkerError};

#[derive(Clone, Debug)]
pub(crate) struct SentryApi<T: Adapter> {
    pub validator: ValidatorDesc,
    pub adapter: T,
    pub sentry_url: String,
    pub client: Client,
    pub logging: bool,
    pub channel: Channel,
}

impl <T : Adapter + 'static> SentryApi <T> {

    pub fn new(adapter: T, channel: &Channel, logging: bool) -> Result<Self, ValidatorWorkerError> {

        let whoami = adapter.whoami();
        let validator = channel.spec.validators.into_iter().find(|&v| v.id == whoami);

        match validator {
            Some(v) => {
                let sentry_url = format!("{}/channel/{}", v.url, channel.id);

                Ok(Self {
                    validator: v.clone().to_owned(),
                    adapter,
                    sentry_url,
                    client: Client::new(),
                    logging,
                    channel: channel.to_owned()
                })

            },
            None => Err(ValidatorWorkerError::InvalidValidatorEntry("we can not find validator entry for whoami".to_string()))
        }
    }

    pub fn all_channels(&self) -> impl Future<Output = Result<Vec<Channel>, Error>> {
        let validator = self.validator.clone();
        // call Sentry and fetch first page, where validator = identity
        let first_page = self.clone().fetch_page(1, validator.clone().url);
        let handle = self.clone();

        first_page
            .and_then(move |response| {
                let first_page_future = ok(response.channels).boxed();

                if response.total_pages < 2 {
                    // if there is only 1 page, return the results
                    first_page_future
                } else {
                    // call Sentry again for the rest of tha pages
                    let futures = (2..=response.total_pages)
                        .map(|page| {
                            handle
                                .clone()
                                .fetch_page(page, validator.clone().url)
                                .map(|response_result| {
                                    response_result.and_then(|response| Ok(response.channels))
                                })
                                .boxed()
                        })
                        .chain(once(first_page_future));

                    try_join_all(futures)
                        .map(|result_all| {
                            result_all
                                .and_then(|all| Ok(all.into_iter().flatten().collect::<Vec<_>>()))
                        })
                        .boxed()
                }
            })
            .boxed()
    }

    async fn fetch_page(
        self,
        page: u64,
        validator: String,
    ) -> Result<ChannelAllResponse, reqwest::Error> {
        
        let mut query = vec![format!("page={}", page)];    
        query.push(format!("validator={}", validator.to_string()));

        let future = self
            .client
            .get(format!("{}/channel/list?{}", self.sentry_url, query.join("&")).as_str())
            .send()
            .and_then(|mut res: Response| res.json::<ChannelAllResponse>());

        await!(future.compat())
    }

    pub fn propagate(&self, messages: Vec<MessageTypes>) {
        let serialised_messages = messages.into_iter().map(
            | message | {
                match message {
                    MessageTypes::NewState(new_state) => serde_json::to_string(&new_state).unwrap(),
                    MessageTypes::ApproveState(approve_state) => serde_json::to_string(&approve_state).unwrap(),
                    MessageTypes::Heartbeat(heartbeat) => serde_json::to_string(&heartbeat).unwrap(),
                    MessageTypes::RejectState(reject_state) => serde_json::to_string(&reject_state).unwrap(),
                    MessageTypes::Accounting(accounting) => serde_json::to_string(&accounting).unwrap()
                }
            }
        ).collect();

        for validator in self.channel.spec.validators.into_iter() {
            let auth = self.adapter.get_auth(&validator);
            auth.and_then( |auth_token| {
                // log if a timeout error occurs
                match propagate_to(auth_token, self.timeout, &validator, &serialised_messages) {
                    Ok(_) => return,
                    Err(e) =>  handle_http_error(e, &validator.url)
                }
            })
            
        }
    }

    pub fn get_latest_msg(from: &str, message_type: &str) -> impl Future<Output = Result<ValidatorMessageResponse, Error>>  {
        let future = self
            .client
            .get(format!("{}/validator-messages/{}/{}?limit=1", self.sentry_url, from, message_type))
            .send()
            .and_then(|mut res: Response| res.json::<ValidatorMessageResponse>());

        await!(future.compat())
    }

    pub fn get_our_latest_msg(message_type: &str) -> impl Future<Output = Result<ValidatorMessageResponse, Error>>  {
        let whoami = adapter.whoami();
        self.get_latest_msg(whoami, message_type)
    }

    pub fn get_last_approved() -> impl Future<Output = Result<LastApprovedResponse, Error>> {
        let future = self
            .client
            .get(format!("{}/last-approved", self.sentry_url))
            .send()
            .and_then(|mut res: Response| res.json::<LastApprovedResponse>());

        await!(future.compat())
    }

    pub fn get_last_msgs() -> impl Future<Output = Result<LastApprovedResponse, Error>>  {
         let future = self
            .client
            .get(format!("{}/last-approved?withHearbeat=true", self.sentry_url))
            .send()
            .and_then(|mut res: Response| res.json::<LastApprovedResponse>());

        await!(future.compat())
    }

    pub fn get_event_aggregates(&self, after: Option<u32> ) -> impl Future<Output = Result<EventAggregateResponse, Error>>  {
        let whoami = adapter.whoami();
        let validator = channel.spec.validators.find(|v| v.id === whoami);
        let auth_token = self.adapter.get_auth(&validator);

        let url = match after {
            Some(duration) => format!("{}/events-aggregates?after={}", self.sentry_url, duration),
            None => format!("{}/events-aggregates", self.sentry_url)
        }
        
        let future = self
            .client
            .get(url)
            .header("authorization", auth_token.to_string())
            .send()
            .and_then(|mut res: Response| res.json::<EventAggregateResponse>());

        await!(future.compat())
    }
}

fn propagate_to(
    auth_token: &str,
    timeout: u32,
    validator: &ValidatorDesc,
    messages: &Vec<String>
    ) -> Result<SuccessResponse, reqwest::Error> {
    // create client with timeout
    let client = reqwest::Client::builder().timeout(Duration::from_secs(timeout)).build()?;
    let url = validator.url.to_string();

    client.post(url)
        .header("authorization".to_string(), auth_token.to_string())    
        .json(messages)
        .send()
}

fn handle_http_error(e: reqwest::Error, url: &str) {
   if e.is_http() {
       match e.url() {
           None => println!("No Url given"),
           Some(url) => println!("Problem making request to: {}", url),
       }
   }
   // Inspect the internal error and output it
   if e.is_serialization() {
      let serde_error = match e.get_ref() {
           None => return,
           Some(err) => err,
       };
       println!("problem parsing information {}", serde_error);
   }

   if e.is_client_error() {
       println!("erorr sending http request for validator {}", url)
   }

   if e.is_redirect() {
       println!("server redirecting too many times or making loop");
   }
}