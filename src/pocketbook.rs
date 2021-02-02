use std::process::Command;

static DIALOG_PATH: &str = "/ebrmain/bin/dialog";

#[allow(dead_code)]
pub enum Icon {
    None = 0,
    Info,
    Question,
    Attention,
    X,
    WLan,
}

pub fn dialog(icon: Icon, text: &str, buttons: &[&str]) -> i32 {
    let iconstr = match icon {
        Icon::None => "0",
        Icon::Info => "1",
        Icon::Question => "2",
        Icon::Attention => "3",
        Icon::X => "4",
        Icon::WLan => "5",
    };

    let res = Command::new(DIALOG_PATH)
        .args([&[iconstr, "", text], buttons].concat())
        .output()
        .unwrap();

    res.status.code().unwrap_or(-1)
}
