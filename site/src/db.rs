use anyhow::Result;
use async_trait::async_trait;
use common::pg::FromClient;
use tokio_postgres::Client;

pub struct Statements {
    pub season: Season,
    pub schedule: Schedule,
    pub show_info: ShowInfo,
    pub show_torrents: ShowTorrents,
    pub unmatched: Unmatched,
    pub new: New,
}

#[async_trait]
impl FromClient for Statements {
    async fn from_client(client: &Client) -> Result<Self> {
        Ok(Self {
            season: Season::new(client).await?,
            schedule: Schedule::new(client).await?,
            show_info: ShowInfo::new(client).await?,
            show_torrents: ShowTorrents::new(client).await?,
            unmatched: Unmatched::new(client).await?,
            new: New::new(client).await?,
        })
    }
}

// language=sql
common::create_statement!(Unmatched, title, trusted, uploaded_at, torrent_id, nyaa_id, hash; "
    select title, trusted, uploaded_at, torrent_id, nyaa_id, hash
    from magnets.torrent
    where not matched and nyaa_id < $1
    order by nyaa_id desc
    limit 101;");

// language=sql
common::create_statement!(ShowTorrents, title, uploaded_at, trusted, torrent_id, hash, nyaa_id; "
    select t.title, t.uploaded_at, t.trusted, t.torrent_id, t.hash, t.nyaa_id
    from magnets.rel_torrent_show rts
    join magnets.torrent t using (torrent_id)
    where rts.show_id = $1 and rts.nyaa_id < $2
    order by rts.nyaa_id desc
    limit 101;");

// language=sql
common::create_statement!(ShowInfo, show_id, anilist_id, season, show_format, names; "
    select
        s.show_id,
        s.anilist_id,
        s.season,
        s.show_format,
        (
            select json_agg(x)
            from (
                select name, show_name_type
                from magnets.show_name
                where show_id = s.show_id and show_name_type in (1, 2)
            ) x
        ) as names
    from magnets.show s
    where s.show_id = $1;");

// language=sql
common::create_statement!(Schedule, schedule_id, show_id, episode, airs_at, names; "
    select
        s.schedule_id,
        s.show_id,
        s.episode,
        s.airs_at,
        (
            select json_agg(x)
            from (
                select name, show_name_type
                from magnets.show_name
                where show_id = s.show_id and show_name_type in (1, 2)
            ) x
        ) as names
    from magnets.schedule s
    where s.airs_at >= $1 and s.airs_at < $2
    order by s.airs_at;");

// language=sql
common::create_statement!(Season, show_id, name, show_name_type; "
    select sn.show_id, sn.name, sn.show_name_type
    from magnets.show_name sn
    join magnets.show s using (show_id)
    where sn.show_name_type in (1, 2) and s.season = $1");

// language=sql
common::create_statement!(New, title, uploaded_at, trusted, torrent_id, hash, nyaa_id; "
    select title, uploaded_at, trusted, torrent_id, hash, nyaa_id
    from magnets.torrent
    where nyaa_id < $1
    order by nyaa_id desc
    limit 101;");
