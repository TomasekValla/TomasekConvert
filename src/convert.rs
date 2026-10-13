//! Rozcestník: podle vstupu a cíle vybere správný nástroj.
use crate::{archive, doc, ffmpeg, formats::*, image, job::*, pdfops};

pub fn run_convert(ctx: &JobCtx, job: &ConvertJob) -> Res<Outcome> {
    match (job.kind, job.fmt.class) {
        (Kind::Pdf, Class::Text) => pdfops::pdf_to_text(ctx, job),
        (Kind::Pdf, Class::Image) => pdfops::pdf_to_images(ctx, job),
        (Kind::Pdf, Class::Doc) => pdfops::pdf_to_doc(ctx, job),
        (Kind::Image, Class::Image) | (Kind::Image, Class::Pdf) => image::convert(ctx, job),
        (Kind::Document, Class::Doc) | (Kind::Document, Class::Pdf) => doc::convert(ctx, job),
        (_, Class::Archive) => archive::convert(ctx, job),
        (_, Class::Video | Class::Audio | Class::AnimImage | Class::FrameImage | Class::Subtitle) => ffmpeg::convert(ctx, job),
        _ => Err(tr!("Tuhle kombinaci vstupu a výstupu zatím neumím", "This input/output combination is not supported yet").into()),
    }
}
