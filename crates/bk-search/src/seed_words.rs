// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! A compact seed vocabulary so typo correction works on a fresh install that
//! has zero bookmarks. The user's own corpus is always merged on top.

pub const SEED_WORDS: &str = "\
the be to of and a in that have i it for not on with he as you do at this but his by from \
they we say her she or an will my one all would there their what so up out if about who get \
which go me when make can like time no just him know take people into year your good some \
could them see other than then now look only come its over think also back after use two how \
our work first well way even new want because any these give day most us is are was were been \
being has had did does doing would should could can may might must shall will am \
home page site link read book bookmarks saved note notes tag tags search find add save \
open close click here there where when why how what which who whom whose \
article blog post news story report guide tutorial review opinion update release version \
download upload install setup config settings option options feature features support help \
document documents documentation docs api server client database query index search engine \
software hardware program application app web website internet network online offline \
computer laptop phone mobile device screen image images photo photos picture video audio \
music file files folder directory path url link links text word words sentence paragraph \
language english word list table column row data value values number numbers count total \
research science study paper papers author authors publisher published publish edition \
design develop development developer code coding program programming function method class \
object variable type value return input output error errors debug test tests testing build \
python rust java javascript typescript html css json sql linux windows mac android ios \
github git commit branch merge pull push repo repository project source open source license \
amazon google microsoft apple meta facebook twitter youtube reddit wikipedia stack overflow \
machine learning neural network model models training dataset data science ai artificial \
intelligence algorithm performance speed memory cpu gpu cache thread threads process \
security password user users account login logout session cookie token private public \
backup restore export import archive compress extract file format json csv markdown pdf \
start stop restart service server status running stopped failed success error warning info \
today yesterday tomorrow week month year hour minute second morning evening night \
good bad best better worst new old big small fast slow high low long short easy hard \
first last next previous again always never sometimes often usually rarely \
yes no maybe please thanks thank sorry hello goodbye welcome \
thing things people person user name title description summary content context \
change changes changed update updates updated create created delete deleted remove removed \
add added find found get got set setting show shown hide hidden view views see seen look looked";

/// Split the seed list into owned words.
pub fn seed_words() -> impl Iterator<Item = (&'static str, u32)> {
    SEED_WORDS.split_whitespace().map(|w| (w, 1u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_is_nontrivial() {
        assert!(SEED_WORDS.split_whitespace().count() > 400);
        assert!(SEED_WORDS.split_whitespace().any(|w| w == "search"));
    }
}
