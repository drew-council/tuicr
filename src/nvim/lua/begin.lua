-- Load a comment into the buffer and place the cursor for Insert mode.
-- Arguments: lines, 0-based row, byte column. Returns true when Insert mode
-- must append after the cursor (`startinsert!`) rather than insert before it.
local lines, row, col = ...
local buffer = vim.g.tuicr_comment_buffer
if not buffer or not vim.api.nvim_buf_is_valid(buffer) then
  error("the tuicr comment buffer no longer exists")
end
if vim.api.nvim_get_current_buf() ~= buffer then
  vim.api.nvim_set_current_buf(buffer)
end

-- Every comment gets its own undo history: a change made while 'undolevels' is
-- -1 discards the old tree, so `u` cannot reach back into the previous one.
local undolevels = vim.bo[buffer].undolevels
vim.bo[buffer].undolevels = -1
vim.api.nvim_buf_set_lines(buffer, 0, -1, false, lines)
vim.bo[buffer].undolevels = undolevels
vim.bo[buffer].modified = false

-- Normal mode cannot place the cursor past the last character, so the end of
-- a non-empty line is reached by appending after it. Insert mode itself is
-- entered by tuicr through nvim_input: started from here it would only take
-- effect after any requests already queued behind this one.
local line = lines[row + 1] or ""
if col >= #line and #line > 0 then
  vim.api.nvim_win_set_cursor(0, { row + 1, #line - 1 })
  return true
end
vim.api.nvim_win_set_cursor(0, { row + 1, col })
return false
