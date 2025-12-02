## Youtube First Comment
A simple program that creates a new comment on YouTube when a new video is published for the specified channel. Once the comment is created, the program exits.

## Installation
```bash
cargo install youtube-first-comment
```

## Usage
```
Usage: yfc [OPTIONS] --google-client-id <GOOGLE_CLIENT_ID> --google-client-secret <GOOGLE_CLIENT_SECRET> --comment <COMMENT> --channel-id <CHANNEL_ID>

Options:
      --google-client-id <GOOGLE_CLIENT_ID>          Google client ID
      --google-client-secret <GOOGLE_CLIENT_SECRET>  Google client secret
      --comment <COMMENT>                            The comment body
      --channel-id <CHANNEL_ID>                      YouTube channel ID
      --poll-interval <POLL_INTERVAL>                Poll interval (in seconds) [default: 60]
      --wait-limit <WAIT_LIMIT>                      Max wait time (in minutes) [optional, defaults to inf]
  -h, --help                                         Print help
```

```bash
yfc --comment "My first comment" \
    --channel-id "<CHANNEL_ID>" \
    --google-client-id "<GOOGLE_CLIENT_ID>" \
    --google-client-secret "<GOOGLE_CLIENT_SECRET>" \
    --poll-interval 0.5 \
    --wait-limit 300
```

You can find the channel id [here](https://www.tunepocket.com/youtube-channel-id-finder) and you will have to create an OAuth 2 Client ID on Google Cloud.
