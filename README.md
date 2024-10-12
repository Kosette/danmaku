# Danmaku

## [中文说明](./README_CN.md)

[mpv](https://mpv.io) danmaku plugin powered by [dandanplay API](https://api.dandanplay.net/swagger/ui/index). The plugin sends the name and hash value of the currently playing file to the dandanplay server to get matching danmaku comments.

## Preview
![image](https://github.com/user-attachments/assets/45ba9fd3-0339-4810-8325-5fcf6800ca24)

## Install

1. build by yourself

```bash
cargo build --release
```

2. download prebuilt package from [release](https://github.com/Kosette/danmaku/releases/latest)

Copy the `.dll`/`.so` file to the `scripts` subdirectory of your mpv configuration directory.

## Usage

Example to bind the `d` key to toggle the danmaku visibility in your `input.conf` (default invisible):

```
d script-message toggle-danmaku
```

It may take some time to load the danmaku after first enabling it.

Set the following options in `script-opts/danmaku.conf` to configure the plugin:

- `font_size=40`: danmaku font size.
- `transparency=48`: 0 (opaque) to 255 (fully transparent).
- `reserved_space=0`: the proportion of reserved space at the bottom of the screen, 0.0 to 1.0 (excluded).
- `speed=1.0`: factor for the speed.
- `no_overlap=yes`: hide the overlapping danmaku, `yes` or `no`.
- `proxy=http://127.0.0.1:8080`: add proxy for requests, default blank
- `user_agent=libmpv`: add user-agent for network requests, default `libmpv`
- `log=false`: `true/on/enable` will enable logging to file, default `false`, log_file `~~/files/danmu.log`
- `filter=keyword1,keyword2`: comma separated keywords, danmaku that contains any of them will be blocked.
- `filter_source=bilibili,gamer`: comma separated case-insensitive sources (`bilibili`, `gamer`, `acfun`, `qq`, `iqiyi`, `d` or `dandan`), danmaku from any of them will be blocked, runtime updatable via `script-opts` option/property.
- `filter_bilibili=~~/files/bilibili.json`: filter file exported from bilibili, regex/user based blocking is not supported, double-tilde placeholders are expanded.

Available script messages:

- `toggle-danmaku`: toggles the danmaku visibility.
- `danmaku-delay <seconds>`: delays danmaku by &lt;seconds&gt; seconds, can be negative.
