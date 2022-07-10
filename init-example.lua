-- load plugins here
local channel = neopult.api.get_channel()

require("channel_banner").setup({ resolution = { 1920, 1080 } })
require("cvh_camera").setup({
    -- You might have to change this
    camera_server_path = "/usr/local/share/cvh-camera/camera-server/dist/server.js",
    -- Change this to match the URL where the camera sender is served
    sender_base_url = "https://your-domain.com/camera-sender.html",
    cameras = 2,
    -- Has to match the view-only password of the vnc channel and has to be public
    janus_room_pin = "testcvh",
    -- This has to match the admin key in the general block of janus.plugin.videoroom.jcfg and should REMAIN PRIVATE
    janus_admin_key = "secret",
})

-- NOTE: Remember to allow tcp traffic on port 5500 + `listen` in your firewall
-- Change this to match the URL/IP to which VNC clients can connect. `:<port>` will be appended to generate the connection url.
local listen_base_url = "your-domain.com"
require("vnc").setup({
    listen = 2 * channel,
    listen_base_url = listen_base_url
})
require("vnc").setup({
    listen = 2 * channel + 1,
    listen_base_url = listen_base_url
})
