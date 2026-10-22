import os

app = defines["app"]
appname = os.path.basename(app)

format = "UDZO"
files = [app]
symlinks = {"Applications": "/Applications"}

icon = defines.get("icon")
background = None
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
window_rect = ((220, 180), (660, 400))
default_view = "icon-view"
icon_size = 100
text_size = 14
icon_locations = {
    appname: (160, 180),
    "Applications": (500, 180),
}
