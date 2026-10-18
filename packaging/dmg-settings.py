# Layout of the .dmg window, for dmgbuild (writes Finder's view settings directly, no Finder needed).
# Used by packaging/dmg.sh; icon positions must match the arrow in packaging/dmg-background.swift.
app = defines["app"]  # noqa: F821 (dmgbuild provides `defines`)
res = defines["res"]  # noqa: F821

format = "UDZO"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = res + "/cctwin.icns"  # icon of the mounted volume
background = res + "/dmg-background.tiff"

window_rect = ((200, 120), (660, 400))
default_view = "icon-view"
icon_size = 128
text_size = 13
icon_locations = {"cctwin.app": (180, 150), "Applications": (480, 150)}
hide_extensions = ["cctwin.app"]
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
