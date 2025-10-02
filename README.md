# liischte
_liischte_ is a vertical wayland bar written in [iced](https://iced.rs/) using the [wlr layer shell](https://wayland.app/protocols/wlr-layer-shell-unstable-v1) protocol that focuses on being lightweight yet useful, and is designed specifically for my own workflow.

> [!IMPORTANT]
> I have written this bar explicitly for my own personal use. I do not intend to give support for other users using _liischte_ on their own system (which is why the issue tab is disabled). This also means that I can make abrupt and breaking changes, or remove stuff entirely whenever I please.
>
> I am making this public as a reference for other people trying to do similar things. Some parts of implementing a bar from scratch in iced have very little documentation, so it can be helpful to have a working implementation to go off of. So use this project more as a reference for your own than using it directly.
>
> Before you copy stuff from here 1:1, you might want to look at [_liischte-lib_](lib/) which is the **toolkit agnostic** collection of data sources _liischte_ is built upon. All features are gated behind crate features so you can theoretically directly depend on parts of it. Note however that I still don't provide any guarantees beyond "it works on my machine".

## showcase
Here's a quick overview of how _liischte_ looks like and what features it has at the time of writing (built from [7770cb9](https://github.com/VirtCode/liischte/commit/7770cb95091473c4aa0b7c1cd0adf8287afd0369))

<div align="center">

https://github.com/user-attachments/assets/2c48beb2-cf5a-4435-b5c2-776a53ace258

</div>

Note that for the features shown in this video, _liischte_ uses less than 25MiB of memory.

## design and features
_liischte_ consists of four fundamentally different parts:
- **core**: It has a few core functions, that being parts of the bar which are more or less hardcoded and only minimally configurable. They currently include the clock and the workspace indicator.
- **status**: A status is an icon which is shown on the bar at all times. It is used to indicate the state of something that's always active, like e.g. the battery information.
- **info**: An info is an icon which is only shown based on certain condition, used to notify the user about less important things on the system. For example whether an idle inhibitor is active.
- **osd**: The osd is a popup above the bar that is shown just momentarily to notify the user about a _change_ to the system. It is not as intrusive as e.g. a notification and is used to for example show volume changes.

Except for the _core_ functionality, the bar is grouped into **modules**. A module usually represents a single source of information that you would like to display on the bar. Modules can all be arranged and induvidually enabled in the config, mainly because different modules are relevant on different hosts (think e.g. battery info).

For a quick overview about what the bar can do, here's a quick list of the features:
- **System Time** shown as simple numerals. It is part of the _core_.
- **Hyprland Workspace** indicator showing available workspaces, their fullscreen and occupied status, and which workspace is currently shown. This is also _core_ functionality, but can be disabled.
- **Battery and AC** information using a _status_ icon which conveys the battery charge accurately.
- **Audio** information showing rough volume settings and mute state as a _status_ with an _osd_ for output and input volume changes and changes to the default devices.
- **Network and Modem** state showing the currently used connection method as a _status_ which supports wifi and cellular signal strength.
- **Backlight** change information with a simple _osd_.
- **Running Processes** which shows an _info_ icon for certain processes defined in the config if they are running.
- **Timers** which are shown as _infos_ and can be dynamically added via the IPC.

## technical decisions
**It is a bar and _not_ a shell.** Things outside that of a bar are explicitly left to other pieces of software. This avoids bloating the scope of this project. For most things in a full shell are either easily done in the terminal or some other projects exist which does mostly what you need (like e.g. [mako](https://github.com/emersion/mako) for notifications). The singular exception to this is the inbuilt OSD, but that is because the bar (e.g. volume changing) would be barely useable without.

**It's written in Rust.** But why? Wouldn't something tailored to the usecase like [ags](https://aylur.github.io/ags) or [quickshell](https://quickshell.outfoxxed.me) have been much easier? Probably, as long as things work and your whole usecase is supported. If things start to break you'll have to start digesting the codebase of your framework which you probably don't know well. And if you want to do things your framework doesn't have the right abstraction for, it is difficult to implement things properly and you quickly see yourself resort to running shell commands in the background instead. Choosing a normal programming language with no framework allows you to  have full control and you can do everything you'd want to. Rust was chosen here mainly because of personal preference.

**The UI is done with Iced.** This is mainly because System76's upcoming desktop environment, [cosmic](https://system76.com/cosmic) uses this library for it's UI. This means there's a high chance that it'll stick around for longer compared to other lesser-known toolkits. It also fits better than GTK and QT because it is designed with Rust in mind. Specifically, we even use (a [fork](https://github.com/VirtCode/iced) of) [cosmic's iced fork](https://github.com/pop-os/iced) because that includes efficient layer surface support which is essential.

## configuration
Configuration is done in `toml` under `$XDG_CONFIG_HOME/.config/liischte.toml`. This section shows available values (with their defaults) mainly as reference for my future self:

```toml
# layer namespace to use (with `-osd` for the osd)
namespace = "liischte"
# whether to show the bar on the left instead of the right
right = false
# output to show the bar on (name, or description with a `desc:` prefix)
# `active` for the active monitor
output = "active"
# whether the ipc socket is enabled
ipc = true

# modules which are enabled
modules = ["power", "audio", "network"]

# looks of the bar
[looks]
    # main foreground color
    foreground = "#FFFFFF"
    # semi-transparent color used for separators etc.
    semi = "#FFFFFF80"
    # main background color for opaque objects (like the osd)
    background = "#000000"
    # border for opaque objects
    border = "#555555"

    # opacity of the background in two-tone icons
    tone_opacity = 0.25

    # font to use for text on the bar
    font = "Jetbrains Mono"

    # padding of the bar to the side
    padding = 10
    # width of the bar
    width = 40

# parameters for the osd
[osd]
    # is the osd enabled
    enabled = true

    # how long to show the osd for an event in millis
    timeout = 4000

    # time the osd hides when respawning in millis
    # this is used such that the compositor has time to show an animation
    respawn_time = 200

# config for the main hyprland widget
[hyprland]
    # enable hyprland workspace indicator
    enabled = true

    # id of the monitor to show workspaces for
    monitor = 0
    # whether to show fullscreen status in bar
    fullscreen = true

    # size of the indicators
    size = 17
    # thickness of the indicator border
    border = 1.5
    # radius of the indicators
    rounding = 6

# config for the main clock widget
[clock]
    # whether to show the seconds indicator
    # (minutes might be inaccurate if disabled)
    seconds = true
```

The rest of the bar consists of different modules which can be enabled and disabled as desired. Basically everything except for the clock and the workspace indicator is a module. Use the above `module` parameter to add or remove a module. Here are the modules which are supported currently.

### `power`
This module shows battery information of the device as a status. It uses `udev` and the `sysfs` under the hood.

```toml
[module.power]
    # force the use of a specific mains supply
    mains =
    # force the use of a specific set of batteries
    batteries = []

    # polling rate to poll battery status in seconds
    polling_rate = 30

    # battery percentage below which it is considered critical
    critical = 0.1
```

### `audio`
This module shows an audio device status and it's changes as an osd. It uses `libpipewire` under the hood.

It does not have any configuration options.

### `network`
This modules shows the network of the device as a status. It uses NetworkManager's and ModemManager's `dbus` interface under the hood.

```toml
[module.network]
    # enables modem manager support
    modem = false
```

### `backlight`
This module shows an osd for brightness changes in the device's baclight. It uses `udev` and the `sysfs` under the hood.

```toml
[module.brightness]
    # force the use of a specific backlight (we use the first one otherwise)
    device =
```

### `process`
This module checks for certain processes running on the system and displays an info indicator for them if found. It uses the `procfs` under the hood.

```toml
[module.process]
    # polling rate to poll processes in seconds
    polling_rate = 600

    # indicators to show based on which processes are running
    indicators = [
        # e.g. { cmdline = "start of cmdline of the process", icon = "icon to show in that case" }
    ]
```

To update this module on demand (to get immediate feedback in the bar despite low polling-rate) you can use pass `rescan` to this module to trigger it to rescan all processes.

This means if you **have ipc support enabled**:
```
liischte pass process rescan
```

### `timer`
This module can show different timers as infos in the bar. The timers can be added on runtime using the ipc and will be tracked by the module. It will display a system notification upon completion of a timer.

**Note: The _visual_ progress of a timer will only update if another widget ticks, this means if the seconds of the clock have been disabled, the visuals might not appear the most responsive. Completion time is accurate in any case.**

```toml
[module.timer]
    # default icon to show if none is set
    default_icon = "alarm-clock"

    # heading to show in the notification
    heading = "Timer Expired!"
    # set notification to never expire
    persistent = true
```

As mentioned, to add a timer to this module, you have to use the IPC. This means you'll need to have **ipc support enabled** if you intend to use this module. The module takes multiple arguments in a special syntax. The arguments supported are:
- `duration` as the duration in seconds
- `icon` (optional) name of the lucide icon to show in the bar
- `message` (optional) message to show in the notification

This means that you can for example run:
```
liischte pass timer "duration=60|icon=soup|message=Your soup is ready to eat."
```

## installation
You can use liischte yourself by building it manually using `cargo build --release`. If you are using [Arch Linux](https://archlinux.org) you can also use the [`PKGBUILD`](PKGBUILD) file found in this repository to build a package with.

## license
This project is licensed under the GPLv3. See the [`LICENSE.md`](LICENSE.md) file for more information.
