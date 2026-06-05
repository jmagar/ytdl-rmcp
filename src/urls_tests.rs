use super::strip_mix_params;

#[test]
fn strips_mix_keeps_video() {
    let got = strip_mix_params(
        "https://www.youtube.com/watch?v=PLEQRIisP_Q&list=RDPLEQRIisP_Q&start_radio=1&pp=abc",
    );
    assert_eq!(got, "https://www.youtube.com/watch?v=PLEQRIisP_Q");
}

#[test]
fn leaves_real_playlist() {
    let u = "https://www.youtube.com/watch?v=abc&list=PLsomething";
    assert_eq!(strip_mix_params(u), u);
}

#[test]
fn leaves_plain_video() {
    let u = "https://www.youtube.com/watch?v=abc";
    assert_eq!(strip_mix_params(u), u);
}

#[test]
fn leaves_non_youtube() {
    let u = "https://vimeo.com/123456";
    assert_eq!(strip_mix_params(u), u);
}
