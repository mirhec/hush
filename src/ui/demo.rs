use crate::model::{Config,Event,Kind,Repo};
use chrono::{Duration,Utc};
pub fn config()->Config {
    Config{login:"alex".into(),repositories:vec![Repo::parse("atelier/design-system").unwrap(),Repo::parse("atelier/web").unwrap()],manual_teams:vec![Repo::parse("atelier/frontend").unwrap()],..Default::default()}
}
pub fn events()->Vec<Event> {
    let rows=[
        (Kind::Request,"A calmer command palette","atelier/design-system","mara","Das Team atelier/frontend wurde um ein Review gebeten.\n\nNeue Tastatur-Navigation, weniger visuelles Rauschen und ein konsistenter Fokuszustand.",4, true),
        (Kind::Mention,"Keyboard navigation in the issue list","atelier/web","jonas","@alex, könntest du dir das Fokusverhalten unter Wayland ansehen? Mit Tab bleibt der Fokus nach dem Schließen des Dialogs noch im Overlay.",18,true),
        (Kind::Review,"Refine the notification preferences","atelier/desktop","lea","Freigegeben.\n\nDie neuen Voreinstellungen sind deutlich verständlicher. Besonders gut: keine privaten Inhalte auf dem Sperrbildschirm.",42,true),
        (Kind::Issue,"Support fractional scaling on Wayland","atelier/design-system","sam","Bei einer Skalierung von 125 % sollten Icons und Trennlinien auf demselben Pixelraster liegen. Beobachtet unter Niri.",78,true),
        (Kind::Mention,"Unify spacing tokens across components","atelier/design-system","mara","@alex, die Abstände sind jetzt auf das gemeinsame 4-Pixel-Raster umgestellt. Was hältst du von der kompakteren Variante?",112,false),
        (Kind::Review,"Keep notification content private by default","atelier/desktop","jonas","Änderungen angefragt.\n\nBitte die Vorschau beim ersten Start ausgeschaltet lassen. Repository-Namen können bereits vertraulich sein.",139,false),
        (Kind::Issue,"Preserve scroll position after filtering","atelier/web","lea","Beim Wechsel des Filters springt die Liste nach oben. Die bisherige Leseposition sollte erhalten bleiben.",1500,false),
        (Kind::Request,"Extract reusable empty states","atelier/design-system","sam","Dir wurde dieser Pull Request zugewiesen.",1680,false),
    ];
    rows.into_iter().enumerate().map(|(i,(kind,title,repository,actor,detail,minutes,unread))|Event{
        id:format!("demo:{i}"),kind,title:title.into(),repository:repository.into(),actor:actor.into(),detail:detail.into(),url:"https://github.com/notifications".into(),occurred_at:Utc::now()-Duration::minutes(minutes),unread,
    }).collect()
}
