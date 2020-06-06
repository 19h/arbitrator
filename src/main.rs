extern crate futures;
extern crate telegram_bot;
extern crate tokio_core;

extern crate rspotify;

use rspotify::spotify::client::Spotify;
use rspotify::spotify::model::playlist::PlaylistTrack;
use rspotify::spotify::model::track::FullTrack;
use rspotify::spotify::model::user::PrivateUser;
use rspotify::spotify::oauth2::{SpotifyClientCredentials, SpotifyOAuth, TokenInfo};

use std::env;
use std::panic;
use std::sync::{Arc, Mutex};

use futures::{Future, Stream};
use telegram_bot::*;
use tokio_core::reactor::Core;

use lazy_static::lazy_static;

lazy_static! {
    static ref SPOTIFY_RGX: regex::Regex = regex::Regex::new(
        r#"(?im)(?:https?://(?:open|play).spotify.com/track/|spotify:track:)(?P<id>.*?)(?:\?.*?)?$"#,
    )
    .unwrap();
    static ref SPOTIFY_TOKEN: Arc<Mutex<TokenInfo>> = Arc::new(Mutex::new(TokenInfo::default()));
}

fn update_spotify_token(
    spotify_client_id: &str,
    spotify_client_secret: &str,
    spotify_root_token: &str,
) {
    if let Some(token_info) = SpotifyOAuth::default()
        .client_id(spotify_client_id)
        .client_secret(spotify_client_secret)
        .scope("user-read-private,user-read-birthdate,user-read-email,playlist-read-private,user-library-read,user-library-modify,user-top-read,playlist-read-collaborative,playlist-modify-public,playlist-modify-private,user-follow-read,user-follow-modify,user-read-playback-state,user-read-currently-playing,user-modify-playback-state,user-read-recently-played")
        .refresh_access_token(spotify_root_token) {
        *SPOTIFY_TOKEN.lock().unwrap() = token_info;
    }
}

fn spotify(
    spotify_client_id: &str,
    spotify_client_secret: &str,
) -> Option<Spotify> {
    let client_credential = SpotifyClientCredentials::default()
        .client_id(spotify_client_id)
        .client_secret(spotify_client_secret)
        .token_info(SPOTIFY_TOKEN.lock().unwrap().clone())
        .build();

    Some(
        Spotify::default()
            .client_credentials_manager(client_credential)
            .build(),
    )
}

fn get_playlist_tracks(
    spotify: &Spotify,
    spotify_user: &str,
    spotify_playlist: &str,
) -> Vec<FullTrack> {
    let mut tracks: Vec<PlaylistTrack> = Vec::new();

    let limit = 100;
    let mut offset = 0;

    loop {
        match spotify.user_playlist_tracks(
            spotify_user,
            spotify_playlist,
            None,
            limit,
            offset,
            None,
        ) {
            Ok(mut track_page) => {
                tracks.append(&mut track_page.items);

                if track_page.total <= (limit + offset) {
                    break;
                }

                offset += limit;
            }
            _ => break,
        }
    }

    tracks
        .into_iter()
        .map(|playlist_track| playlist_track.track)
        .collect()
}

fn main() {
    let spotify_user = env::var("SPOTIFY_USER").unwrap();
    let spotify_playlist = env::var("SPOTFIY_PLAYLIST").unwrap();

    let spotify_client_id = env::var("SPOTIFY_CLIENT_ID").unwrap();
    let spotify_client_secret = env::var("SPOTIFY_CLIENT_SECRET").unwrap();

    let spotify_root_token = env::var("SPOTFIY_ROOT_TOKEN").unwrap();

    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(1800u64));

        panic::catch_unwind(
            || {
                let spotify_client_id = env::var("SPOTIFY_CLIENT_ID").unwrap();
                let spotify_client_secret = env::var("SPOTIFY_CLIENT_SECRET").unwrap();

                let spotify_root_token = env::var("SPOTFIY_ROOT_TOKEN").unwrap();

                update_spotify_token(
                    &spotify_client_id.clone(),
                    &spotify_client_secret.clone(),
                    &spotify_root_token.clone(),
                );
            },
        );
    });

    update_spotify_token(
        &spotify_client_id.clone(),
        &spotify_client_secret.clone(),
        &spotify_root_token.clone(),
    );

    let mut core = Core::new().unwrap();

    let token = env::var("TELEGRAM_BOT_TOKEN").unwrap();
    let api = Api::configure(token).build(core.handle()).unwrap();

    let future =
        api.stream().for_each(|update| {
            if let UpdateKind::Message(message) = update.kind {
                if let MessageKind::Text { ref data, .. } = message.kind {
                    //println!(
                    //    "<{}; {}>: {}",
                    //    &message.from.id, &message.from.first_name, data
                    //);

                    let captures = SPOTIFY_RGX.captures(data);

                    if captures.is_none() {
                        return Ok(());
                    }

                    let id = captures.unwrap().name("id");

                    if !id.is_some() {
                        return Ok(());
                    }

                    let id = id.unwrap().as_str();

                    spotify(
                        &spotify_client_id,
                        &spotify_client_secret,
                    )
                        .map(|spotify| {
                            let tracks = get_playlist_tracks(
                                &spotify,
                                &spotify_user,
                                &spotify_playlist,
                            );

                            let track_match = tracks
                                .into_iter()
                                .enumerate()
                                .find(
                                    |(i, track)|
                                        match &track.id {
                                            Some(id_string) => id_string.as_str() == id,
                                            None => false
                                        }
                                );

                            match track_match {
                                Some((i, _)) => {
                                    if i != 0 {
                                        spotify.user_playlist_recorder_tracks(
                                            &spotify_user,
                                            &spotify_playlist,
                                            i as i32,
                                            1,
                                            0,
                                            None,
                                        );

                                        api.spawn(message.text_reply(
                                            "Track already in playlist, moved it to the top.",
                                        ));
                                    }
                                }
                                None => {
                                    spotify.user_playlist_add_tracks(
                                        &spotify_user,
                                        &spotify_playlist,
                                        &[id.into()],
                                        Some(0),
                                    );

                                    api.spawn(
                                        message.text_reply("Track added to playlist!"),
                                    );
                                }
                            }
                        });
                }
            }

            Ok(())
        });

    core.run(future).unwrap();
}

