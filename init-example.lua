-- load plugins here
local channel = neopult.api.get_channel()

require("channel_banner").setup({ resolution = { 1920, 1080 } })
require("cvh_camera").setup({
    -- You might have to change this
    camera_server_path = "/usr/local/share/cvh-camera/camera-server/dist/server.js",
    -- Change this to match the domain where the camera sender is served
    sender_base_url = "https://your-domain.com/camera-sender.html",
    cameras = 2,
    -- Has to match the view-only password of the vnc channel
    janus_room_pin = "testcvh",
})
require("vnc").setup({ listen = 2 * channel })
require("vnc").setup({ listen = 2 * channel + 1 })
