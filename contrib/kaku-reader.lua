-- Optional tmd native-reader adapter. Install by registering this module once
-- in your Kaku config. It does not modify key bindings or shell configuration.
local wezterm = require 'wezterm'
local M = {}

function M.setup()
  local requests = {}
  wezterm.on('user-var-changed', function(window, pane, name, value)
    if name ~= 'TMD_READER' then return end
    local action, token, reply = value:match('^v1:(%a+):([0-9a-f]+):(.*)$')
    if not action then action, token = value:match('^v1:(%a+):([0-9a-f]+)$') end
    if not action or #token > 64 then return end
    local id = pane:pane_id()
    if action == 'probe' then
      local suffix = '/tmd-reader-' .. token .. '/reply'
      if not reply or reply:sub(1, 1) ~= '/' or reply:sub(-#suffix) ~= suffix then return end
      local capacity = math.max(0, window:effective_config().scrollback_lines)
      -- Read an existing protocol marker before appending. Never recreate a
      -- timed-out request directory and never inject replies into keyboard input.
      local input = io.open(reply, 'rb')
      if not input then return end
      local marker = input:read('*l')
      input:close()
      if marker ~= 'tmd-reader-v1:' .. token then return end
      local output = io.open(reply, 'r+b')
      if not output then return end
      requests[id] = { token = token, row = pane:get_cursor_position().y }
      output:seek('end')
      output:write(string.format('%.0f\n', capacity))
      output:close()
    elseif action == 'show' then
      local request = requests[id]
      if not request or request.token ~= token then return end
      requests[id] = nil
      local dims = pane:get_dimensions()
      window:perform_action(wezterm.action.ScrollToBottom, pane)
      window:perform_action(wezterm.action.ScrollByLine(
        math.max(request.row, dims.scrollback_top) - dims.physical_top), pane)
    end
  end)
end

return M
