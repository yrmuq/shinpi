use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone)]
pub enum Command {
    Toggle,
    Dismiss,
    Refresh,
    Settings,
    Quit,
    CheckUpdate,
    TestNotification,
    AboutOpened,
    AutoCheckUpdates(bool),
    OpenLink(String),
    LogoTapped,
    Metrics(u32, u32, u32),
    ResetAppearance,
    ResetDev,
    LockWindowSize(bool),
    SettingsResized(u32, u32),
    Add,
    Select(usize),
    Edit(usize),
    Remove(usize),
    SaveEdit(String, usize),
    Language(usize),
    Theme(i32),
    Orientation(u8),
    Colored(bool),
    Notifications(bool),
    Threshold(u8),
    Repeat(u32),
    PanelWidth(u32),
    HistoryCount(u32),
    ToggleHistory,
    DismissNotice(usize),
}

pub struct Tray {
    pub sender: UnboundedSender<Command>,
    pub icons: Vec<ksni::Icon>,
    pub summary: String,
    pub labels: [String; 3],
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "shinpi".into()
    }
    fn title(&self) -> String {
        "shinpi".into()
    }
    fn category(&self) -> ksni::Category {
        ksni::Category::Hardware
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.clone()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "shinpi".into(),
            description: self.summary.clone(),
            ..Default::default()
        }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.sender.send(Command::Toggle);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        [
            (self.labels[0].clone(), Command::Refresh),
            (self.labels[1].clone(), Command::Settings),
            (self.labels[2].clone(), Command::Quit),
        ]
        .into_iter()
        .map(|(label, command)| {
            ksni::menu::StandardItem {
                label,
                activate: Box::new(move |this: &mut Self| {
                    let _ = this.sender.send(command.clone());
                }),
                ..Default::default()
            }
            .into()
        })
        .collect()
    }
}
