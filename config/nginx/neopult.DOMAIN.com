# IMPORTANT: Replace all occurrences of `neopult.DOMAIN.com` with your actual
# domain/subdomain! Make sure to also provide valid SSL certificates for this
# domain.

server {
    listen 443 ssl;
    server_name neopult.DOMAIN.com;

    root /var/www/neopult;

    # Static files for noVNC.
    location /novnc {
        alias /usr/share/novnc/;
    }

    # Static files for the CVH-Camera sender.
    #
    # Not really necessary when root is /var/www/neopult already.
    location /sender {
        alias /var/www/neopult/sender;
    }

    # yesVNC static files.
    location /yesvnc {
        alias /usr/local/share/yesvnc/public;
    }

    # Websockify for yesVNC on channel 5.
    location /websockify/105 {
        proxy_read_timeout 10800s;
        proxy_pass http://localhost:6185;
    }

    # Websockify for noVNC.
    # 
    # This matches /websockify/1, /websockify/2, ..., /websockify/6 via a regex.
    #
    # Be careful to add ^ at the beginning and $ at the end, or this will also
    # match `/websockify/105` for example, because the number starts with [1-6].
    #
    # You can also choose to set these blocks up manually. For example, for
    # channel 5:
    #
    # ```
    # location /websockify/5 {
    #     proxy_read_timeout 10800s;
    #     proxy_pass http://localhost:6085;
    # }
    # ```
    location ~ ^/websockify/([1-6])$ {
        proxy_read_timeout 10800s;
        # `127.0.0.1` has to be used, because nginx won't resolve `localhost` when
        # variables are in there proxy address
        proxy_pass http://127.0.0.1:608$1;
    }

    # Socket.io endpoints for CVH-Camera.
    # 
    # See the comment above for more information on the regex matching.
    location ~ ^/socket\.io/500([1-6])/(.*)$ {
        proxy_pass http://127.0.0.1:500$1/socket.io/$2$is_args$args;
    }

    # Proxy for the admin APIs of the Neopult instances.
    #
    # See comment above for more information on the regex matching.
    #
    # If you choose to not use the regex matching, you can also set these up
    # manually. For example, for channel 5 (note the trailing slahes):
    # ```
    # location /5/ {
    #     proxy_pass http://localhost:4205/;
    # }
    # ```
    location ~ ^/([1-6])/(.*)$ {
        proxy_pass http://127.0.0.1:420$1/$2$is_args$args;
    }

    # Svelte admin frontend for the Neopult instances.
    location /svelte-frontend {
        alias /usr/local/share/neopult/neopult/svelte/build;
    }

    # Neopult Lighthouse static files.
    location /static {
        alias /usr/local/share/neopult/neopult-lighthouse/static;
    }

    # Neopult Lighthouse server.
    location / {
        proxy_pass http://localhost:4199;
    }
}

# Redirect HTTP to HTTPS.
server {
    listen 80;
    server_name neopult.DOMAIN.com;
    return 301 https://$host$request_uri;
}

# Reverse proxy for Janus WebRTC server.
#
# Make sure to allow TCP traffic on port 8089 in your firewall.
server {
    server_name _;

    location /janus {
        proxy_pass http://127.0.0.1:8088/janus;
    }

    listen 8089 ssl;
}
