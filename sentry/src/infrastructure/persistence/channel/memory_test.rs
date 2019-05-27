use chrono::Utc;
use time::Duration;
use uuid::Uuid;

use crate::domain::{Channel, ChannelListParams, ChannelRepository, RepositoryError};
use crate::domain::fixtures::*;

use super::MemoryChannelRepository;

#[test]
fn initializes_with_channels_and_lists_channels() {
    futures::executor::block_on(async {
        let valid_until_ge = Utc::now() - Duration::days(1);

        let empty_init = MemoryChannelRepository::new(None);
        let params = ChannelListParams::new(valid_until_ge, 10, 1, None).unwrap();
        assert_eq!(0, await!(empty_init.list(&params)).expect("Empty initial list").len());

        let channels = [get_channel("channel 1", &None, None), get_channel("channel 2", &None, None)];
        // this shouldn't change the order in any way
        let some_init = MemoryChannelRepository::new(Some(&channels));

        let channels_list: Vec<Channel> = await!(some_init.list(&params)).expect("List the initial 2 channels");
        assert_eq!(2, channels_list.len());

        let last_channel = channels_list.last().expect("There should be a last Channel (total: 2)");
        assert_eq!(get_channel_id("channel 2"), last_channel.id);
    })
}

#[test]
fn listing_channels_can_handle_page_and_limit() {
    futures::executor::block_on(async {
        let valid_until_ge = Utc::now() - Duration::days(1);

        // using Utc::now() will assure that the channels always have >= valid_until_ge DateTime
        let channels = get_channels(6, Some(Utc::now()));

        let repository = MemoryChannelRepository::new(Some(&channels));

        // check if we will get all channels, using a limit > channels count
        let params = ChannelListParams::new(valid_until_ge, 10, 1, None).unwrap();
        let all_channels = await!(repository.list(&params)).expect("Should list all channels");
        assert_eq!(6, all_channels.len());

        // also check if we are getting the correct last channel for the page
        assert_eq!(&get_channel_id("channel 6"), &all_channels[5].id);

        // check if we will get the first 4 channels on page 1, if the limit is 4
        let params = ChannelListParams::new(valid_until_ge, 4, 1, None).unwrap();
        let first_page_three_channels = await!(repository.list(&params)).unwrap();
        assert_eq!(4, first_page_three_channels.len());

        // also check if we are getting the correct last channel for the page
        assert_eq!(&get_channel_id("channel 4"), &first_page_three_channels[3].id);

        // if we have 5 per page & we are on page 2, one is left
        let params = ChannelListParams::new(valid_until_ge, 5, 2, None).unwrap();
        let one_channel_on_page = await!(repository.list(&params)).unwrap();
        assert_eq!(1, one_channel_on_page.len());

        // also check if we are getting the last channel for the page
        assert_eq!(&get_channel_id("channel 6"), &one_channel_on_page[0].id);

        // if we are out of bound, sort of speak - we have 6 channels, limit 6, so we have only 1 page
        // we should get 0 channels on page 2
        let params = ChannelListParams::new(valid_until_ge, 6, 2, None).unwrap();
        assert_eq!(0, await!(repository.list(&params)).unwrap().len());

        // if we have limit 2 and we are on page 2, we should get 2 channels back
        let params = ChannelListParams::new(valid_until_ge, 2, 2, None).unwrap();
        let two_channels_on_page = await!(repository.list(&params)).unwrap();
        assert_eq!(2, two_channels_on_page.len());

        assert_eq!(&get_channel_id("channel 3"), &two_channels_on_page[0].id);
        assert_eq!(&get_channel_id("channel 4"), &two_channels_on_page[1].id);
    })
}

#[test]
fn listing_channels_can_handle_valid_until_filtration() {
    futures::executor::block_on(async {
        let valid_until_yesterday = Some(Utc::now() - Duration::days(1));
        // create the valid_until_ge, before creating the channels,
        // as they might otherwise have valid_until < valid_until_ge
        let valid_until_ge = Utc::now();

        let channels = [
            get_channel("channel 1", &None, None),
            get_channel("channel 2 yesterday", &valid_until_yesterday, None),
            get_channel("channel 3", &None, None),
            get_channel("channel 4 yesterday", &valid_until_yesterday, None),
            get_channel("channel 5", &None, None),
        ];

        let repository = MemoryChannelRepository::new(Some(&channels));

        let params = ChannelListParams::new(valid_until_ge, 10, 1, None).unwrap();
        let list_channels = await!(repository.list(&params)).expect("Should list all channels");

        assert_eq!(3, list_channels.len());

        assert_eq!(&get_channel_id("channel 1"), &list_channels[0].id);
        assert_eq!(&get_channel_id("channel 3"), &list_channels[1].id);
        assert_eq!(&get_channel_id("channel 5"), &list_channels[2].id);
    })
}

#[test]
fn listing_channels_can_handles_validator_filtration_and_keeps_valid_until_filtration() {
    futures::executor::block_on(async {
        let valid_until_yesterday = Some(Utc::now() - Duration::days(1));
        // create the valid_until_ge, before creating the channels,
        // as they might otherwise have valid_until < valid_until_ge
        let valid_until_ge = Utc::now();

        let validators = vec![
            get_validator("validator-1"),
            get_validator("validator-2"),
        ];
        let validators_opt = ValidatorsOption::Some(validators);
        let channel_2_spec = get_channel_spec(Uuid::new_v4(), validators_opt.clone());
        let channel_5_spec = get_channel_spec(Uuid::new_v4(), validators_opt.clone());

        let channels = [
            get_channel("channel 1", &None, None),
            get_channel("channel 2 yesterday", &valid_until_yesterday, Some(channel_2_spec)),
            get_channel("channel 3", &None, None),
            get_channel("channel 4 yesterday", &valid_until_yesterday, None),
            get_channel("channel 5", &None, Some(channel_5_spec)),
        ];

        let repository = MemoryChannelRepository::new(Some(&channels));

        let params = ChannelListParams::new(valid_until_ge, 10, 1, Some("validator-1".to_string())).unwrap();
        let list_channels = await!(repository.list(&params)).expect("Should list all channels");

        assert_eq!(1, list_channels.len());
        assert_eq!(&get_channel_id("channel 5"), &list_channels[0].id);
    })
}

#[test]
fn saves_channels() {
    futures::executor::block_on(async {
        let valid_until_ge = Utc::now() - Duration::days(1);

        let channels = [get_channel("XYZ", &None, None)];

        let some_init = MemoryChannelRepository::new(Some(&channels));

        // get a 2nd channel to save
        let new_channel = get_channel("ABC", &None, None);

        // save the 2nd channel
        // this shouldn't change the order in any way
        await!(some_init.save(new_channel)).expect("Saving 2nd new channel");

        let params = ChannelListParams::new(valid_until_ge, 10, 1, None).unwrap();
        let channels_list: Vec<Channel> = await!(some_init.list(&params)).expect("List the 2 total channels");
        assert_eq!(2, channels_list.len());

        let last_channel = channels_list.last().expect("There should be a last Channel (total: 2)");
        assert_eq!(get_channel_id("ABC"), last_channel.id);

        // get a 3rd channel to save
        let new_channel = get_channel("DEF", &None, None);

        // save the 2nd channel
        // this shouldn't change the order in any way
        await!(some_init.save(new_channel)).expect("Saving 3rd new channel");

        let channels_list: Vec<Channel> = await!(some_init.list(&params)).expect("List the 3 total channels");
        assert_eq!(3, channels_list.len());

        let last_channel = channels_list.last().expect("There should be a last Channel (total: 3)");
        assert_eq!(get_channel_id("DEF"), last_channel.id);
    })
}

#[test]
fn saving_the_same_channel_id_should_error() {
    futures::executor::block_on(async {
        let channels = [get_channel("ABC", &None, None)];

        let repository = MemoryChannelRepository::new(Some(&channels));

        let same_channel_id = get_channel("ABC", &None, None);

        let error = await!(repository.save(same_channel_id)).expect_err("It shouldn't be possible to save the same channel_id");

        match error {
            RepositoryError::User => {}
            _ => panic!("Expected UserError"),
        }
    })
}