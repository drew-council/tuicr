-- One-time setup of the embedded Neovim that edits tuicr comments.
-- Arguments: RPC channel id, comment tab width.
local channel, tab_width = ...

if vim.fn.has("nvim-0.10") == 0 then
  error("tuicr needs Neovim 0.10 or newer")
end

-- tuicr draws the comment box, mode badge, and command line itself, so none of
-- Neovim's own chrome is wanted.
vim.o.showmode = false
vim.o.ruler = false
vim.o.laststatus = 0

-- No UI is attached, so forward messages to tuicr's message bar instead of
-- letting them pile up behind a hit-enter prompt nobody can see.
local function message_text(content)
  local chunks = {}
  for _, chunk in ipairs(content or {}) do
    chunks[#chunks + 1] = tostring(chunk[2] or "")
  end
  return (table.concat(chunks):gsub("\r\n?", "\n"))
end

local namespace = vim.api.nvim_create_namespace("TuicrMessages")
local ok, err = pcall(vim.ui_attach, namespace, { ext_messages = true }, function(event, ...)
  if event == "msg_show" then
    local kind, content = ...
    local text = message_text(content)
    -- "return_prompt" is the empty hit-enter prompt itself and "search_cmd"
    -- echoes the search being run; neither carries information.
    if text ~= "" and kind ~= "return_prompt" and kind ~= "search_cmd" then
      vim.rpcnotify(channel, "tuicr_message", kind, text)
    end
  end
end)
if not ok then
  vim.schedule(function()
    vim.rpcnotify(channel, "tuicr_message", "wmsg", "Neovim message capture unavailable: " .. tostring(err))
  end)
end

local buffer = vim.api.nvim_create_buf(false, true)
vim.g.tuicr_comment_buffer = buffer
vim.api.nvim_buf_set_name(buffer, "tuicr://comment")
-- `acwrite` routes `:w` through BufWriteCmd, which saves the comment.
vim.bo[buffer].buftype = "acwrite"
vim.bo[buffer].bufhidden = "hide"
vim.bo[buffer].swapfile = false
vim.bo[buffer].undofile = false
vim.api.nvim_set_current_buf(buffer)
vim.bo[buffer].filetype = "markdown"
-- Applied after the filetype so a markdown ftplugin cannot override them: Tab
-- inserts `comment_tab_width` spaces, the same as the built-in vim box.
vim.bo[buffer].expandtab = true
vim.bo[buffer].tabstop = tab_width
vim.bo[buffer].softtabstop = tab_width
vim.bo[buffer].shiftwidth = tab_width

local group = vim.api.nvim_create_augroup("TuicrComment", { clear = true })

-- tuicr re-reads the state after every key it forwards. These cover changes it
-- did not cause, such as a mapping that resolves after 'timeoutlen'.
vim.api.nvim_create_autocmd({
  "TextChanged",
  "TextChangedI",
  "TextChangedP",
  "CursorMoved",
  "CursorMovedI",
  "ModeChanged",
  "CmdlineChanged",
  "RecordingEnter",
  "RecordingLeave",
  "BufEnter",
}, {
  group = group,
  callback = function()
    vim.rpcnotify(channel, "tuicr_dirty")
  end,
})

vim.api.nvim_create_autocmd("BufWriteCmd", {
  group = group,
  buffer = buffer,
  callback = function()
    vim.bo[buffer].modified = false
    vim.rpcnotify(channel, "tuicr_save", vim.api.nvim_buf_get_lines(buffer, 0, -1, false))
  end,
})

-- `:q` cancels the comment. Clearing 'modified' first keeps it from failing
-- with E37 after edits; `:wq` has already saved through BufWriteCmd.
vim.api.nvim_create_autocmd("QuitPre", {
  group = group,
  callback = function()
    if vim.api.nvim_buf_is_valid(buffer) then
      vim.bo[buffer].modified = false
    end
  end,
})

-- Quitting the last window exits Neovim. tuicr treats that as a cancel and
-- starts a fresh instance for the next comment.
vim.api.nvim_create_autocmd("VimLeavePre", {
  group = group,
  once = true,
  callback = function()
    vim.rpcnotify(channel, "tuicr_quit")
  end,
})

return true
