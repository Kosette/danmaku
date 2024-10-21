# Danmaku

[mpv](https://mpv.io) 通过 [dandanplay API](https://api.dandanplay.net/swagger/ui/index) 驱动的弹幕插件。插件会将当前播放文件的名称和哈希值发送到 dandanplay 服务器，获取匹配的弹幕评论，可以和 Emby 搭配使用。

<b>插件在某些情况下会匹配失效，此为正常现象。</b>

## 安装

1. 自行编译：

```bash
cargo build --release
```

2. 从 [release](https://github.com/Kosette/danmaku/releases/latest) 中下载对应平台文件

将 `.dll`/`.so` 文件复制到 mpv 配置目录下的 `scripts` 子目录中。

## 使用

在`input.conf`中绑定热键，默认弹幕不加载：

```
CTRL+d script-message toggle-danmaku // CTRL+d 打开弹幕，按键绑定可自由更换
```

如果你使用uosc UI框架，可以在uosc.conf中的`controls`字段添加`<video>command:clear_all:script-message toggle-danmaku?弹幕开关`，给Danmaku添加一个按钮。

开启后需要一些时间加载弹幕。

在 `script-opts/danmaku.conf` 中设置以下选项以配置插件：

- `font_size=40`：弹幕字体大小。
- `transparency=48`：0（不透明）到 255（完全透明）。
- `reserved_space=0`：底部保留空间的比例，0.0 到 1.0（不包括 1.0）。
- `speed=1.0`：弹幕速度。
- `no_overlap=yes`：隐藏重叠的弹幕，`yes` 或 `no`。
- `proxy=http://127.0.0.1:8080`：为请求添加代理，**默认为空**。
- `user_agent=libmpv`：为网络请求添加用户代理，默认为 `libmpv`
- `log=false`: `true/on/enable` 开启输出日志到文件，默认`false`，日志文件 `~~/files/danmu.log`
- `filter=keyword1,keyword2`：逗号分隔的关键字，弹幕过滤。
- `filter_source=bilibili,gamer`：逗号分隔的大小写不敏感来源（`bilibili`、`gamer`、`acfun`、`qq`、`iqiyi`、`d` 或 `dandan`），过滤弹幕来源，可在运行时通过 `script-opts` 选项/属性更新。
- `filter_bilibili=~~/files/bilibili.json`：从 bilibili 导出的弹幕屏蔽过滤器文件，不支持基于 正则/用户的规则，双波浪符占位符将被扩展。

可用的脚本消息/script-message：

- `toggle-danmaku`：切换弹幕可见性。
- `danmaku-delay <seconds>`：通过 &lt;seconds&gt; 秒延迟弹幕，可为负数。
