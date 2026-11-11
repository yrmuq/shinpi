use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone, Copy)]
pub enum Command {
    Toggle,
    Dismiss,
    Refresh,
    Settings,
    Quit,
}

pub struct Tray {
    pub sender: UnboundedSender<Command>,
    pub icons: Vec<ksni::Icon>,
    pub summary: String,
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "shinpi".into()
    }
    fn title(&self) -> String {
        "shinpi".into()
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
            ("Обновить", Command::Refresh),
            ("Настройки", Command::Settings),
            ("Выйти", Command::Quit),
        ]
        .into_iter()
        .map(|(label, command)| {
            ksni::menu::StandardItem {
                label: label.into(),
                activate: Box::new(move |this: &mut Self| {
                    let _ = this.sender.send(command);
                }),
                ..Default::default()
            }
            .into()
        })
        .collect()
    }
}
