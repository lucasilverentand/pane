use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionMode {
    None,
    Char,
    Line,
    Block,
}

pub enum CopyModeAction {
    None,
    YankSelection(String),
    Exit,
}

#[allow(dead_code)]
pub struct CopyModeState {
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub selection_start: Option<(usize, usize)>,
    pub selection_mode: SelectionMode,
    pub search_query: String,
    pub search_matches: Vec<(usize, usize, usize)>, // (row, col_start, col_end)
    pub search_active: bool,
    pub screen_rows: usize,
    pub screen_cols: usize,
    pub scroll_offset: usize,
}

impl CopyModeState {
    pub fn new(screen_rows: usize, screen_cols: usize, cursor_row: usize, cursor_col: usize) -> Self {
        Self {
            cursor_row,
            cursor_col,
            selection_start: None,
            selection_mode: SelectionMode::None,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_active: false,
            screen_rows,
            screen_cols,
            scroll_offset: 0,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, screen: &vt100::Screen) -> CopyModeAction {
        if self.search_active {
            return self.handle_search_key(key, screen);
        }

        let mods = key.modifiers;
        match key.code {
            KeyCode::Esc => {
                if self.selection_mode != SelectionMode::None {
                    self.selection_start = None;
                    self.selection_mode = SelectionMode::None;
                    CopyModeAction::None
                } else {
                    CopyModeAction::Exit
                }
            }
            KeyCode::Char('q') if mods.is_empty() => CopyModeAction::Exit,

            // Cursor movement
            KeyCode::Char('h') | KeyCode::Left if mods.is_empty() => {
                self.move_left();
                CopyModeAction::None
            }
            KeyCode::Char('j') | KeyCode::Down if mods.is_empty() => {
                self.move_down(screen);
                CopyModeAction::None
            }
            KeyCode::Char('k') | KeyCode::Up if mods.is_empty() => {
                self.move_up();
                CopyModeAction::None
            }
            KeyCode::Char('l') | KeyCode::Right if mods.is_empty() => {
                self.move_right(screen);
                CopyModeAction::None
            }

            // Word movement
            KeyCode::Char('w') if mods.is_empty() => {
                self.move_word_forward(screen);
                CopyModeAction::None
            }
            KeyCode::Char('b') if mods.is_empty() => {
                self.move_word_backward(screen);
                CopyModeAction::None
            }

            // Line start/end
            KeyCode::Char('0') if mods.is_empty() => {
                self.cursor_col = 0;
                CopyModeAction::None
            }
            KeyCode::Char('$') if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                self.move_to_line_end(screen);
                CopyModeAction::None
            }

            // Top/bottom
            KeyCode::Char('g') if mods.is_empty() => {
                self.cursor_row = 0;
                self.cursor_col = 0;
                CopyModeAction::None
            }
            KeyCode::Char('G') if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                let max_row = self.max_row(screen);
                self.cursor_row = max_row;
                self.cursor_col = 0;
                CopyModeAction::None
            }

            // Half-page movement
            KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => {
                let half = self.screen_rows / 2;
                self.cursor_row = self.cursor_row.saturating_sub(half);
                CopyModeAction::None
            }
            KeyCode::Char('d') if mods.contains(KeyModifiers::CONTROL) => {
                let half = self.screen_rows / 2;
                let max_row = self.max_row(screen);
                self.cursor_row = (self.cursor_row + half).min(max_row);
                CopyModeAction::None
            }

            // Selection
            KeyCode::Char('v') if mods.is_empty() => {
                self.toggle_selection(SelectionMode::Char);
                CopyModeAction::None
            }
            KeyCode::Char('V') if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                self.toggle_selection(SelectionMode::Line);
                CopyModeAction::None
            }
            KeyCode::Char('v') if mods.contains(KeyModifiers::CONTROL) => {
                self.toggle_selection(SelectionMode::Block);
                CopyModeAction::None
            }

            // Yank
            KeyCode::Char('y') if mods.is_empty() => {
                if self.selection_mode != SelectionMode::None {
                    let text = self.selected_text(screen);
                    if !text.is_empty() {
                        return CopyModeAction::YankSelection(text);
                    }
                }
                CopyModeAction::None
            }

            // Search
            KeyCode::Char('/') if mods.is_empty() => {
                self.search_active = true;
                self.search_query.clear();
                CopyModeAction::None
            }
            KeyCode::Char('n') if mods.is_empty() => {
                self.next_match();
                CopyModeAction::None
            }
            KeyCode::Char('N') if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                self.prev_match();
                CopyModeAction::None
            }

            _ => CopyModeAction::None,
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent, screen: &vt100::Screen) -> CopyModeAction {
        match key.code {
            KeyCode::Esc => {
                self.search_active = false;
                self.search_query.clear();
                self.search_matches.clear();
                CopyModeAction::None
            }
            KeyCode::Enter => {
                self.search_active = false;
                self.perform_search(screen);
                self.next_match();
                CopyModeAction::None
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                CopyModeAction::None
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                CopyModeAction::None
            }
            _ => CopyModeAction::None,
        }
    }

    fn move_left(&mut self) {
        self.cursor_col = self.cursor_col.saturating_sub(1);
    }

    fn move_right(&mut self, screen: &vt100::Screen) {
        let max_col = self.line_end_col(screen, self.cursor_row);
        if self.cursor_col < max_col {
            self.cursor_col += 1;
        }
    }

    fn move_up(&mut self) {
        self.cursor_row = self.cursor_row.saturating_sub(1);
    }

    fn move_down(&mut self, screen: &vt100::Screen) {
        let max_row = self.max_row(screen);
        if self.cursor_row < max_row {
            self.cursor_row += 1;
        }
    }

    fn move_to_line_end(&mut self, screen: &vt100::Screen) {
        self.cursor_col = self.line_end_col(screen, self.cursor_row);
    }

    fn move_word_forward(&mut self, screen: &vt100::Screen) {
        let max_row = self.max_row(screen);
        let line = self.get_line_text(screen, self.cursor_row);
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor_col;
        let mut row = self.cursor_row;

        // Skip current word (non-space chars)
        while col < chars.len() && !chars[col].is_whitespace() {
            col += 1;
        }
        // Skip spaces
        while col < chars.len() && chars[col].is_whitespace() {
            col += 1;
        }

        if col >= chars.len() && row < max_row {
            // Move to next line start
            row += 1;
            col = 0;
            let next_line = self.get_line_text(screen, row);
            let next_chars: Vec<char> = next_line.chars().collect();
            while col < next_chars.len() && next_chars[col].is_whitespace() {
                col += 1;
            }
        }

        self.cursor_row = row;
        self.cursor_col = col;
    }

    fn move_word_backward(&mut self, screen: &vt100::Screen) {
        let line = self.get_line_text(screen, self.cursor_row);
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor_col;
        let mut row = self.cursor_row;

        if col == 0 && row > 0 {
            row -= 1;
            let prev_line = self.get_line_text(screen, row);
            col = prev_line.trim_end().len();
        }

        if col > 0 {
            col -= 1;
            let ln = self.get_line_text(screen, row);
            let cs: Vec<char> = ln.chars().collect();
            // Skip spaces
            while col > 0 && col < cs.len() && cs[col].is_whitespace() {
                col -= 1;
            }
            // Skip word chars
            while col > 0 && col < cs.len() && !cs[col - 1].is_whitespace() {
                col -= 1;
            }
        }
        let _ = chars; // used implicitly via col bounds
        self.cursor_row = row;
        self.cursor_col = col;
    }

    fn toggle_selection(&mut self, mode: SelectionMode) {
        if self.selection_mode == mode {
            self.selection_mode = SelectionMode::None;
            self.selection_start = None;
        } else {
            self.selection_mode = mode;
            self.selection_start = Some((self.cursor_row, self.cursor_col));
        }
    }

    pub fn selected_text(&self, screen: &vt100::Screen) -> String {
        let (start_row, start_col) = match self.selection_start {
            Some(s) => s,
            None => return String::new(),
        };

        match self.selection_mode {
            SelectionMode::None => String::new(),
            SelectionMode::Char => {
                self.extract_char_selection(screen, start_row, start_col)
            }
            SelectionMode::Line => {
                self.extract_line_selection(screen, start_row)
            }
            SelectionMode::Block => {
                self.extract_block_selection(screen, start_row, start_col)
            }
        }
    }

    fn extract_char_selection(
        &self,
        screen: &vt100::Screen,
        start_row: usize,
        start_col: usize,
    ) -> String {
        let (sr, sc, er, ec) = self.normalize_range(start_row, start_col);
        let mut result = String::new();

        for row in sr..=er {
            let line = self.get_line_text(screen, row);
            let chars: Vec<char> = line.chars().collect();
            let from = if row == sr { sc } else { 0 };
            let to = if row == er { (ec + 1).min(chars.len()) } else { chars.len() };
            let segment: String = chars[from.min(chars.len())..to.min(chars.len())].iter().collect();
            if row > sr {
                result.push('\n');
            }
            result.push_str(&segment);
        }
        result
    }

    fn extract_line_selection(&self, screen: &vt100::Screen, start_row: usize) -> String {
        let (sr, er) = if start_row <= self.cursor_row {
            (start_row, self.cursor_row)
        } else {
            (self.cursor_row, start_row)
        };

        let mut lines = Vec::new();
        for row in sr..=er {
            lines.push(self.get_line_text(screen, row).trim_end().to_string());
        }
        lines.join("\n")
    }

    fn extract_block_selection(
        &self,
        screen: &vt100::Screen,
        start_row: usize,
        start_col: usize,
    ) -> String {
        let (sr, sc, er, ec) = self.normalize_range(start_row, start_col);
        let mut lines = Vec::new();
        for row in sr..=er {
            let line = self.get_line_text(screen, row);
            let chars: Vec<char> = line.chars().collect();
            let from = sc.min(chars.len());
            let to = (ec + 1).min(chars.len());
            let segment: String = chars[from..to].iter().collect();
            lines.push(segment);
        }
        lines.join("\n")
    }

    fn normalize_range(
        &self,
        start_row: usize,
        start_col: usize,
    ) -> (usize, usize, usize, usize) {
        if (start_row, start_col) <= (self.cursor_row, self.cursor_col) {
            (start_row, start_col, self.cursor_row, self.cursor_col)
        } else {
            (self.cursor_row, self.cursor_col, start_row, start_col)
        }
    }

    fn perform_search(&mut self, screen: &vt100::Screen) {
        self.search_matches.clear();
        if self.search_query.is_empty() {
            return;
        }
        let query = &self.search_query;
        let rows = screen.size().0 as usize;
        for row in 0..rows {
            let line = self.get_line_text(screen, row);
            let mut start = 0;
            while let Some(pos) = line[start..].find(query) {
                let col_start = start + pos;
                let col_end = col_start + query.len().saturating_sub(1);
                self.search_matches.push((row, col_start, col_end));
                start = col_start + 1;
            }
        }
    }

    fn next_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        let cur = (self.cursor_row, self.cursor_col);
        let next = self
            .search_matches
            .iter()
            .find(|m| (m.0, m.1) > cur)
            .or_else(|| self.search_matches.first());
        if let Some(&(row, col, _)) = next {
            self.cursor_row = row;
            self.cursor_col = col;
        }
    }

    fn prev_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        let cur = (self.cursor_row, self.cursor_col);
        let prev = self
            .search_matches
            .iter()
            .rev()
            .find(|m| (m.0, m.1) < cur)
            .or_else(|| self.search_matches.last());
        if let Some(&(row, col, _)) = prev {
            self.cursor_row = row;
            self.cursor_col = col;
        }
    }

    fn get_line_text(&self, screen: &vt100::Screen, row: usize) -> String {
        let cols = screen.size().1 as usize;
        let mut line = String::with_capacity(cols);
        for col in 0..cols {
            if let Some(cell) = screen.cell(row as u16, col as u16) {
                let contents = cell.contents();
                if contents.is_empty() {
                    line.push(' ');
                } else {
                    line.push_str(&contents);
                }
            } else {
                line.push(' ');
            }
        }
        line
    }

    fn line_end_col(&self, screen: &vt100::Screen, row: usize) -> usize {
        let line = self.get_line_text(screen, row);
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            0
        } else {
            trimmed.len().saturating_sub(1)
        }
    }

    fn max_row(&self, screen: &vt100::Screen) -> usize {
        screen.size().0 as usize - 1
    }

    /// Check if a cell is within the current selection.
    pub fn is_selected(&self, row: usize, col: usize) -> bool {
        let (start_row, start_col) = match self.selection_start {
            Some(s) => s,
            None => return false,
        };
        match self.selection_mode {
            SelectionMode::None => false,
            SelectionMode::Char => {
                let (sr, sc, er, ec) = self.normalize_range(start_row, start_col);
                if row < sr || row > er {
                    return false;
                }
                if sr == er {
                    col >= sc && col <= ec
                } else if row == sr {
                    col >= sc
                } else if row == er {
                    col <= ec
                } else {
                    true
                }
            }
            SelectionMode::Line => {
                let (sr, er) = if start_row <= self.cursor_row {
                    (start_row, self.cursor_row)
                } else {
                    (self.cursor_row, start_row)
                };
                row >= sr && row <= er
            }
            SelectionMode::Block => {
                let (sr, sc, er, ec) = self.normalize_range(start_row, start_col);
                row >= sr && row <= er && col >= sc && col <= ec
            }
        }
    }

    /// Check if a cell is a search match.
    pub fn is_search_match(&self, row: usize, col: usize) -> bool {
        self.search_matches
            .iter()
            .any(|&(mr, mc_start, mc_end)| row == mr && col >= mc_start && col <= mc_end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn make_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_screen(rows: u16, cols: u16, text: &str) -> vt100::Parser {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(text.as_bytes());
        parser
    }

    #[test]
    fn test_new_state() {
        let state = CopyModeState::new(24, 80, 5, 10);
        assert_eq!(state.cursor_row, 5);
        assert_eq!(state.cursor_col, 10);
        assert_eq!(state.selection_mode, SelectionMode::None);
        assert!(!state.search_active);
    }

    // Cursor movement
    #[test]
    fn test_move_left() {
        let parser = make_screen(5, 20, "hello world");
        let mut state = CopyModeState::new(5, 20, 0, 5);
        state.handle_key(make_key(KeyCode::Char('h'), KeyModifiers::NONE), parser.screen());
        assert_eq!(state.cursor_col, 4);
    }

    #[test]
    fn test_move_left_at_zero() {
        let parser = make_screen(5, 20, "hello");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        state.handle_key(make_key(KeyCode::Char('h'), KeyModifiers::NONE), parser.screen());
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_move_right() {
        let parser = make_screen(5, 20, "hello world");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        state.handle_key(make_key(KeyCode::Char('l'), KeyModifiers::NONE), parser.screen());
        assert_eq!(state.cursor_col, 1);
    }

    #[test]
    fn test_move_down() {
        let parser = make_screen(5, 20, "line1\r\nline2");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        state.handle_key(make_key(KeyCode::Char('j'), KeyModifiers::NONE), parser.screen());
        assert_eq!(state.cursor_row, 1);
    }

    #[test]
    fn test_move_up() {
        let parser = make_screen(5, 20, "line1\r\nline2");
        let mut state = CopyModeState::new(5, 20, 1, 0);
        state.handle_key(make_key(KeyCode::Char('k'), KeyModifiers::NONE), parser.screen());
        assert_eq!(state.cursor_row, 0);
    }

    #[test]
    fn test_move_up_at_zero() {
        let parser = make_screen(5, 20, "hello");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        state.handle_key(make_key(KeyCode::Char('k'), KeyModifiers::NONE), parser.screen());
        assert_eq!(state.cursor_row, 0);
    }

    // Line start/end
    #[test]
    fn test_move_to_start() {
        let parser = make_screen(5, 20, "hello world");
        let mut state = CopyModeState::new(5, 20, 0, 5);
        state.handle_key(make_key(KeyCode::Char('0'), KeyModifiers::NONE), parser.screen());
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_move_to_end() {
        let parser = make_screen(5, 20, "hello world");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        state.handle_key(make_key(KeyCode::Char('$'), KeyModifiers::NONE), parser.screen());
        assert_eq!(state.cursor_col, 10); // last char of "hello world"
    }

    // Top/bottom
    #[test]
    fn test_move_to_top() {
        let parser = make_screen(5, 20, "a\r\nb\r\nc");
        let mut state = CopyModeState::new(5, 20, 2, 0);
        state.handle_key(make_key(KeyCode::Char('g'), KeyModifiers::NONE), parser.screen());
        assert_eq!(state.cursor_row, 0);
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_move_to_bottom() {
        let parser = make_screen(5, 20, "a\r\nb\r\nc");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        state.handle_key(make_key(KeyCode::Char('G'), KeyModifiers::SHIFT), parser.screen());
        assert_eq!(state.cursor_row, 4); // screen has 5 rows (0-4)
    }

    // Word movement
    #[test]
    fn test_word_forward() {
        let parser = make_screen(5, 20, "hello world foo");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        state.handle_key(make_key(KeyCode::Char('w'), KeyModifiers::NONE), parser.screen());
        assert_eq!(state.cursor_col, 6); // start of "world"
    }

    #[test]
    fn test_word_backward() {
        let parser = make_screen(5, 20, "hello world foo");
        let mut state = CopyModeState::new(5, 20, 0, 6);
        state.handle_key(make_key(KeyCode::Char('b'), KeyModifiers::NONE), parser.screen());
        assert_eq!(state.cursor_col, 0); // start of "hello"
    }

    // Selection
    #[test]
    fn test_char_selection_toggle() {
        let parser = make_screen(5, 20, "hello");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        state.handle_key(make_key(KeyCode::Char('v'), KeyModifiers::NONE), parser.screen());
        assert_eq!(state.selection_mode, SelectionMode::Char);
        assert_eq!(state.selection_start, Some((0, 0)));

        // Toggle off
        state.handle_key(make_key(KeyCode::Char('v'), KeyModifiers::NONE), parser.screen());
        assert_eq!(state.selection_mode, SelectionMode::None);
        assert_eq!(state.selection_start, None);
    }

    #[test]
    fn test_line_selection_toggle() {
        let parser = make_screen(5, 20, "hello");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        state.handle_key(make_key(KeyCode::Char('V'), KeyModifiers::SHIFT), parser.screen());
        assert_eq!(state.selection_mode, SelectionMode::Line);
    }

    #[test]
    fn test_block_selection_toggle() {
        let parser = make_screen(5, 20, "hello");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        state.handle_key(make_key(KeyCode::Char('v'), KeyModifiers::CONTROL), parser.screen());
        assert_eq!(state.selection_mode, SelectionMode::Block);
    }

    #[test]
    fn test_char_selection_yank() {
        let parser = make_screen(5, 20, "hello world");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        // Start selection at 0
        state.handle_key(make_key(KeyCode::Char('v'), KeyModifiers::NONE), parser.screen());
        // Move right 4 times to select "hello"
        for _ in 0..4 {
            state.handle_key(make_key(KeyCode::Char('l'), KeyModifiers::NONE), parser.screen());
        }
        let action = state.handle_key(make_key(KeyCode::Char('y'), KeyModifiers::NONE), parser.screen());
        match action {
            CopyModeAction::YankSelection(text) => assert_eq!(text, "hello"),
            _ => panic!("Expected YankSelection"),
        }
    }

    #[test]
    fn test_line_selection_yank() {
        let parser = make_screen(5, 20, "hello\r\nworld\r\nfoo");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        // Line selection
        state.handle_key(make_key(KeyCode::Char('V'), KeyModifiers::SHIFT), parser.screen());
        // Move down to include second line
        state.handle_key(make_key(KeyCode::Char('j'), KeyModifiers::NONE), parser.screen());
        let action = state.handle_key(make_key(KeyCode::Char('y'), KeyModifiers::NONE), parser.screen());
        match action {
            CopyModeAction::YankSelection(text) => assert_eq!(text, "hello\nworld"),
            _ => panic!("Expected YankSelection"),
        }
    }

    // Selection query
    #[test]
    fn test_is_selected_char() {
        let mut state = CopyModeState::new(5, 20, 0, 2);
        state.selection_start = Some((0, 0));
        state.selection_mode = SelectionMode::Char;
        assert!(state.is_selected(0, 0));
        assert!(state.is_selected(0, 1));
        assert!(state.is_selected(0, 2));
        assert!(!state.is_selected(0, 3));
    }

    #[test]
    fn test_is_selected_line() {
        let mut state = CopyModeState::new(5, 20, 1, 0);
        state.selection_start = Some((0, 0));
        state.selection_mode = SelectionMode::Line;
        assert!(state.is_selected(0, 5));
        assert!(state.is_selected(1, 0));
        assert!(!state.is_selected(2, 0));
    }

    #[test]
    fn test_is_selected_block() {
        let mut state = CopyModeState::new(5, 20, 2, 4);
        state.selection_start = Some((0, 2));
        state.selection_mode = SelectionMode::Block;
        assert!(state.is_selected(1, 3));
        assert!(!state.is_selected(1, 5));
        assert!(!state.is_selected(3, 3));
    }

    // Search
    #[test]
    fn test_search_enter_and_exit() {
        let parser = make_screen(5, 20, "hello world");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        state.handle_key(make_key(KeyCode::Char('/'), KeyModifiers::NONE), parser.screen());
        assert!(state.search_active);
        state.handle_key(make_key(KeyCode::Esc, KeyModifiers::NONE), parser.screen());
        assert!(!state.search_active);
    }

    #[test]
    fn test_search_finds_match() {
        let parser = make_screen(5, 40, "hello world hello");
        let mut state = CopyModeState::new(5, 40, 0, 0);
        // Enter search
        state.handle_key(make_key(KeyCode::Char('/'), KeyModifiers::NONE), parser.screen());
        state.handle_key(make_key(KeyCode::Char('w'), KeyModifiers::NONE), parser.screen());
        state.handle_key(make_key(KeyCode::Char('o'), KeyModifiers::NONE), parser.screen());
        state.handle_key(make_key(KeyCode::Char('r'), KeyModifiers::NONE), parser.screen());
        state.handle_key(make_key(KeyCode::Char('l'), KeyModifiers::NONE), parser.screen());
        state.handle_key(make_key(KeyCode::Char('d'), KeyModifiers::NONE), parser.screen());
        // Confirm
        state.handle_key(make_key(KeyCode::Enter, KeyModifiers::NONE), parser.screen());
        assert!(!state.search_active);
        assert_eq!(state.search_matches.len(), 1);
        assert_eq!(state.cursor_col, 6); // jumped to "world"
    }

    #[test]
    fn test_search_next_prev() {
        let parser = make_screen(5, 40, "aa bb aa bb aa");
        let mut state = CopyModeState::new(5, 40, 0, 0);
        state.search_query = "aa".to_string();
        state.perform_search(parser.screen());
        assert_eq!(state.search_matches.len(), 3);

        // Cursor at (0,0) which is the first match position, so next_match
        // skips it and goes to the second match
        state.next_match();
        assert_eq!(state.cursor_col, 6);
        state.next_match();
        assert_eq!(state.cursor_col, 12);
        // Wrap around
        state.next_match();
        assert_eq!(state.cursor_col, 0);

        // Prev match from (0,0) wraps to last
        state.prev_match();
        assert_eq!(state.cursor_col, 12);
    }

    #[test]
    fn test_is_search_match() {
        let parser = make_screen(5, 20, "hello world");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        state.search_query = "world".to_string();
        state.perform_search(parser.screen());

        assert!(state.is_search_match(0, 6));
        assert!(state.is_search_match(0, 10));
        assert!(!state.is_search_match(0, 0));
        assert!(!state.is_search_match(0, 11));
    }

    // Esc exits selection first, then copy mode
    #[test]
    fn test_esc_clears_selection_first() {
        let parser = make_screen(5, 20, "hello");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        state.handle_key(make_key(KeyCode::Char('v'), KeyModifiers::NONE), parser.screen());
        assert_eq!(state.selection_mode, SelectionMode::Char);
        let action = state.handle_key(make_key(KeyCode::Esc, KeyModifiers::NONE), parser.screen());
        assert!(matches!(action, CopyModeAction::None));
        assert_eq!(state.selection_mode, SelectionMode::None);
        // Now Esc exits
        let action = state.handle_key(make_key(KeyCode::Esc, KeyModifiers::NONE), parser.screen());
        assert!(matches!(action, CopyModeAction::Exit));
    }

    #[test]
    fn test_exit_with_q() {
        let parser = make_screen(5, 20, "hello");
        let mut state = CopyModeState::new(5, 20, 0, 0);
        let action = state.handle_key(make_key(KeyCode::Char('q'), KeyModifiers::NONE), parser.screen());
        assert!(matches!(action, CopyModeAction::Exit));
    }

    #[test]
    fn test_half_page_up() {
        let parser = make_screen(10, 20, "");
        let mut state = CopyModeState::new(10, 20, 8, 0);
        state.handle_key(make_key(KeyCode::Char('u'), KeyModifiers::CONTROL), parser.screen());
        assert_eq!(state.cursor_row, 3); // 8 - 10/2 = 3
    }

    #[test]
    fn test_half_page_down() {
        let parser = make_screen(10, 20, "");
        let mut state = CopyModeState::new(10, 20, 2, 0);
        state.handle_key(make_key(KeyCode::Char('d'), KeyModifiers::CONTROL), parser.screen());
        assert_eq!(state.cursor_row, 7); // 2 + 10/2 = 7
    }

    // Block selection
    #[test]
    fn test_block_selection_yank() {
        let parser = make_screen(5, 20, "abcde\r\nfghij\r\nklmno");
        let mut state = CopyModeState::new(5, 20, 0, 1);
        state.handle_key(make_key(KeyCode::Char('v'), KeyModifiers::CONTROL), parser.screen());
        // Move to row 1, col 3
        state.handle_key(make_key(KeyCode::Char('j'), KeyModifiers::NONE), parser.screen());
        state.handle_key(make_key(KeyCode::Char('l'), KeyModifiers::NONE), parser.screen());
        state.handle_key(make_key(KeyCode::Char('l'), KeyModifiers::NONE), parser.screen());
        let action = state.handle_key(make_key(KeyCode::Char('y'), KeyModifiers::NONE), parser.screen());
        match action {
            CopyModeAction::YankSelection(text) => assert_eq!(text, "bcd\nghi"),
            _ => panic!("Expected YankSelection"),
        }
    }
}
