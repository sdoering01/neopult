# Neopult

> This documentation is not yet finished. Quotes starting with "TODO" indicate
> work that is yet to be done.

Neopult is an extensible and customizable synchronous teaching platform. It is
primarily suited for online university lectures with one presenter.

Neopult can be extended and customized via plugins written in the Lua scripting
language. The [default plugins](./neopult/plugins) contain functionality to
show a banner, when a virtual room ("channel") is currently unused, transmit
screen contents via VNC and share the presenter's webcam. Teachers can also
give their students the possibility to transmit their webcam and screen
contents. This could be useful for presentations of student projects for
example.

Neopult explicitly does not contain the possibility to transmit audio. Since
audio is obviously necessary for online lectures, we recommend
[Mumble](https://www.mumble.info/) for that.

Neopult is heavily inspired by
[PULT](https://gitlab.cvh-server.de/pgerwinski/pult), which was written by
Peter Gerwinski during the COVID-19 pandemic. While PULT had to be developed
under time pressure to keep lectures at our campus going, Neopult was written
from the ground up with the ideas and learnings from PULT in mind.

## Technical Overview

Since robustness is a high priority of Neopult, Rust was chosen as the language
of its core. At the same time, Neopult provides a Lua API to provide
customizability and extensibility via a plugin system.

### Channels

Multiple Neopult instances can run at the same time. Each instance is
responsible for one *channel*. A channel is identified by its channel number
(currently soft-limited to integers from 0 to 99) and can be compared to a
physical room. Meaning, one lecture or class should preferably happen in the
same channel, so that students (or listeners in general) can easily find them.

In production, a separate user (e.g. `neopult`) should run all Neopult
instances. The user's home is called *Neopult home*. The Neopult home contains
directories for each channel in the form of `channel-<channel_number>` -- the
so called *channel homes*. One given Neopult instance knows for which channel
it is responsible through the `NEOPULT_CHANNEL` environment variable, that has
to be set accordingly. The channel home can contain additional files that are
required for that channel to work (e.g. configuration files for programs that
are executed by plugins -- more on that later). On startup, the Neopult
instance loads a `init.lua` file from its channel home, that contains plugins
and configuration for that specific channel.

### Virtual Displays

Neopult relies heavily on X11 (and other Linux software). For that reason, it
currently only runs on Linux.

Each Neopult instance requires its own virtual X display. This display can be
seen as the canvas on which plugins can draw content. They do so, by launching
plain GUI applications. To make the display (and its content) visible for the
viewers, a VNC server is used. Each VNC server instance also poses as a X
display. To watch the screen content, one needs a VNC client. We use the VNC
web client [noVNC](https://novnc.com/info.html), to make things easier for the
users. This way, only a web browser is needed to follow an online lecture.

With the current plugins, the display may contain a channel banner (which is
essentially a pdf viewer in fullscreen mode), and listening VNC clients. These
listening VNC clients can themselves show the contents of the (real) screens of
people in the lecture. Essentially, the screen content of a real person is
shown in a window on the virtual X display. And because the virtual X display
is transmitted to the viewers, so is the screen content.

The Neopult core provides a window manager that manages all the windows on the
virtual display.

#### Window Manager

Each window that is managed by Neopult's window manager can be in one of three
modes: *max*, *min* and *hidden*. To position the windows, the window manager
first draws the window that was most recently put to *max* (called the *primary
window*) in the background. The primary window takes up the whole display by
default. The window manager then layers all windows in *min* mode on top of the
primary window. Min windows usually only take up a portion of the display.
Windows that are *hidden*, are not shown.

An example may better explain this concept: Say, you have two people that are
holding a lecture and have one shared set of presentation slides. Both would
share their face cam and one would share their screen. The window with the
screen's contents would be put to *max* mode, and the camera feeds would be put
to *min* mode. This results in two camera feeds that are layered on top of the
presentation slides.

### Configuration and Plugins

Before loading the `init.lua` file, Neopult places Lua functions inside of the
global Lua context. These functions pose the API and hook right into the
Neopult core. The `init.lua` file can use those functions for configuration and
creating plugin instances.

The complete Lua API is documented with type hints
[here](./neopult/neopult.lua). Nevertheless, some basic concepts are still
discussed in this document.

#### Plugins and Plugin Instances

To create a plugin, one should first write a separate Lua script. The Lua
script should return a Lua table that contains a `setup` function, which should
accept a single Lua table that contains arguments to configure the plugin's
behavior. The `setup` function of the plugin should register a new *plugin
instance* with Neopult via the API. One can think of plugins as classes and the
`setup` function as the constructor that creates a new distinct object. The
corresponding API function `register_plugin_instance` returns a plugin instance
handle on success. The plugin can use that handle to start processes, claim and
manage windows, perform plugin-scoped logging, and to register modules.

#### Modules

A *module* provides a way for admins to interact with plugins and vice versa. A
module can be created by using the `register_module` function of a plugin
instance handle. On success, the function returns a module handle. A plugin
instance can register multiple modules or can even choose to not register a
module at all. For example, the plugin that provides the channel banner does
the latter, since it does not need to provide any interactivity.

Modules consist of multiple parts:

- **Actions** can be triggered by admins to control the plugin. Module handles
  can register actions with the `register_action` function, by providing a name
  and a callback. When the action is triggered by an admin, the callback is
  executed. Actions can also be set as active via the module's
  `set_active_actions` function. The provided web interface highlights those
  actions.

  The plugin that transmits camera feeds provides the actions "Start", "Stop",
  "Min", "Max" and "Hide" for example. Note that the last three correspond to
  the modes of the window manager, since they control the (virtual) window,
  that shows the camera feed. The current mode of that window is reflected via
  the active action. For example, when the window is in *max* mode, the "Max"
  action is set to active and thus highlighted.

- A **status** that signals the general state of the module. The camera plugin
  uses the statuses "inactive", "waiting" and "active". These statuses are also
  assigned a special color in the provided admin web interface, so that admins
  can quickly get a glimpse of all the modules' statuses.

- A **message** that can be freely set by the module, to notify the admins
  about something. The camera plugin uses the message to provide a link to the
  admin, that can be used to share ones camera feed. The admin could also send
  this link to somebody else, so they can share their camera feed.

#### Plugin Design Considerations

When creating a new plugin that needs to have multiple modules, one has two
choices:

- Create a single plugin instance and register multiple modules via that
  instance (i.e. call the plugin's `setup` function once)
- Create multiple plugin instances and register one module via each instance
  (i.e. call the plugin's `setup` function multiple times)

The first former choice should be used, if the modules share some resource
(e.g. a process). This is the case for the camera plugin, since it spawns a
process that controls all camera feeds. Since one might want to control the
camera feeds individually, each camera feed has its own module. Using this
choice, allows each module to communicate with the control process.

If there are no shared resources, the latter choice is preferred. This way,
each plugin instance and its module can be configured via the arguments of the
plugin's `setup` function. This provides more flexibility on the usage side of
the plugin (i.e. in the `init.lua`). The plugin that transmits screen contents
uses this approach, since the modules do not need access to a shared resource.

#### Spawning Processes and Claiming Windows

If a plugin wants to shows something on the virtual display, it first has to
spawn a corresponding GUI application, by using the plugin instance handle's
`spawn_process` function. This function returns a process handle on success.
Plugins can listen to the output (stdout and stderr) of a process by providing
a callback to `spawn_process`. They can also write to the stdin of the process
via the process handle.

After the GUI application is started, the plugin has to inform Neopult about
the new window, so that its window manager can manage it. To do that, the
plugin can use the `claim_window` function of the plugin instance handle. This
function requires a (sub)string of the window's class (`WM_CLASS` atom) as an
argument. The function tries to find an unmanaged window whose class contains
this argument. Additionally, the plugin can provide a *min geometry* to the
function, that defines the window's geometry in the min mode.

The plugin can now use the window handle to put the window into the three
supported modes (max, min and hide).

### Websocket Admin API and Web Interface

Neopult provides a websocket admin API to manage all the modules that were
registered via the plugins. This API can be used to trigger the actions of
modules and it also provides live updates of the modules' state (i.e. their
status, message and active actions). The websocket admin API is protected by a
password to guard against abuse. The password can be set via the
`neopult.config.websocket_password` variable inside the `init.lua` script.

> Note, that the password has to be set after the `init.lua` script is
> evaluated (meaning, that it should not be set inside of a callback), and that
> it cannot be changed during the lifetime of the Neopult instance. If you want
> to change the password, you have to restart the corresponding Neopult
> instance.

Neopult provides an admin web interface that consumes the websocket admin API.
The web interface is implemented with Svelte and styled with TailwindCSS. Its
source can be found [here](./neopult/svelte).

### Neopult Lighthouse

Neopult Lighthouse is a way to make Neopult channels discoverable. It is a
separate application, that inspects the Neopult home, renders a corresponding
HTML overview and serves that overview via HTTP. The provided channel overview
lists all Neopult channels with two links to view the channel (via noVNC) and
to go the admin web interface of that channel. Neopult Lighthouse periodically
checks for changes in the Neopult home and rerenders the channel overview, if
necessary.

In the case, that you want hidden channels (e.g. for private meetings), it
usually does not make sense to show these channels in a public channel
overview. To hide such channels, you can create a file named `lighthouse_hide`
in the root of the channel home.

Neopult Lighthouse can be configured via CLI flags. The available flags can be
inspected by running the program with the `--help` flag. The source of Neopult
Lighthouse can be found [here](./neopult-lighthouse).

### Transmission of Camera Feeds

To transmit camera feeds, Neopult uses another project made by us:
[CVH-Camera](https://gitlab.cvh-server.de/sdoering/cvh-camera). CVH-Camera
relies on the WebRTC Server [Janus](https://janus.conf.meetecho.com/), and
poses as an adapter between Neopult (or PULT for that matter) and Janus.

> "CVH" stands for "Campus Velbert/Heiligenhaus", which is the university
> campus at which this project was developed.

CVH-Camera provides a camera server that controls all the logical slots for the
camera feeds. This camera server provides an interface via its stdin/stdout and
a named pipe (`mkfifo`). Neopult provides a plugin that uses this interface to
make the functionality of CVH-Camera available in the admin web interface. That
plugin also spawns the camera server process.

For a more in-depth explanation of CVH-Camera, refer to its
[README](https://gitlab.cvh-server.de/sdoering/cvh-camera/-/blob/master/README.md).

## Installation

> It is recommended to read the technical overview first, in case you have
> skipped it.

> As explained in the technical overview, Neopult only runs on Linux for now,
> since it relies heavily on X11 and VNC. However, viewers and admins can use
> any operating system of their choice.

The following installation instructions are written for Ubuntu 20.04. But they
should also work for more recent Ubuntu versions (or other Linux
distributions), although some small adjustments may be required (e.g. config
directories, ...).

### Dependencies

First, install the following dependencies via apt:

- `tigervnc`: Provides the VNC server for the virtual display and the listening
  client that displays screen contents of participants.
- `zathura`: Pdf viewer that is used by the channel banner plugin.
- `novnc`: VNC web client that is used to display the virtual display.
- `janus`: WebRTC Server that is used to transmit camera feeds.
- `nodejs`: Javascript runtime that is used by CVH-Camera, to control logical
  slots for camera feeds. This also install the node package manager (`npm`).
- `coreutils` (should be installed by default): Collection of utility programs.
  The utility `tail` is used in the CVH-Camera plugin.

Neopult itself is written in Rust, so while you are at it, you should also
install the Rust toolchain. The recommended way is to use `rustup`. Please
refer to the [official Rust installation
instructions](https://www.rust-lang.org/tools/install) to install it.

#### Nginx

Since most of the used applications have HTTP interfaces, but don't natively
support HTTPS, it is mandatory to use a reverse proxy in front of those
applications in order to secure them! **If you do not do this, all HTTP traffic
(including passwords, camera feeds and screen contents) will be sent over the
internet without any kind of encryption and can thus be read and modified by
third parties**! We recommend the usage of nginx as the webserver and reverse
proxy, but you can use any webserver or reverse proxy of your liking. You can
issue HTTPS certificates for your domain via [Let's
Encrypt](https://letsencrypt.org/). The process, of how you issue those
certificates and use them with nginx is beyond the scope of this documentation,
but there are many good guides on the internet.

An example configuration for nginx can be found
[here](./config/nginx/neopult.DOMAIN.com). It should be renamed to the actual
domain you are using and placed inside of the directory
`/etc/nginx/sites-available`. Then you should symlink (`ln -s`) that file to
the directory `/etc/nginx/sites-enabled`.

Inside of the config, you have to change all occurrences of
`neopult.DOMAIN.com` to your actual domain. Make sure, that you provide a SSL
certificate for that domain in your main nginx config at
`/etc/nginx/nginx.conf`.

#### CVH-Camera

For the transmission of camera feeds, you will also need to download the latest
version of CVH-Camera. You can find its git repository
[here](https://gitlab.cvh-server.de/sdoering/cvh-camera). The recommended
installation path is `/usr/local/share/cvh-camera`.

Since CVH-Camera is written in Typescript, you first need to compile it, in
order to execute it. To do this, go to
`/usr/local/share/cvh-camera/camera-server` and run `npm install`, followed by
`npm run build`.

##### Janus

CVH-Camera creates a room via the videoroom Janus plugin on the fly. To do
that, it uses Janus' admin API. This API should obviously not be open to the
internet, thus Janus provides an admin key, that has to be sent with every
request to the admin API. It is highly recommended to set that admin key to
something secret. You only have to type it into the config of Janus and the
`init.lua` file of Neopult, so you can use a long random string.

To change the admin key, open the config of the Janus videoroom plugin at
`/etc/janus/janus.plugin.videoroom.jcfg`. Under the `general` block, you should
see a line that contains `#admin_key = "..."`. Remove the `#` to activate the
admin key and insert your randomly generated key between the quotes.

**IMPORTANT**: Keep in mind to also provide the same admin key to the
`cvh_camera` plugin in the `init.lua`, via the `janus_admin_key` parameter.

**NOTE**: Make sure to allow TCP traffic on port 8089 in your firewall.

##### Patch noVNC

> TODO: Add instruction to inject camera viewer scripts into noVNC

#### yesVNC

Usually, people that want to share their screen are required to download a
corresponding software. To make things easier, we also provide a simple web
client to share ones screen via the browser:
[yesVNC](https://github.com/sdoering01/yesvnc). Most of yesVNC was written by
Peter Gerwinski. If you want to use it, download the latest version from its
repository. The recommended installation path is `/usr/local/share/yesvnc`





> TODO: Installation, Directory Locations

> TODO: Set Janus admin_key in Janus config and init.lua

> TODO: Neopult Lighthouse Config and flags in systemd service
>   - To do this: New systemd service for neopult lighthouse

> TODO: yesVNC

> TODO: Open the ports in the firewall for VNC

> TODO: Update neopult.lua
