begin;

-- drop schema if exists magnets cascade;

create schema magnets;

create table magnets.show_format (
    show_format int primary key,
    description text not null,
    created timestamptz not null default now()
);

insert into magnets.show_format
values
       (1, 'tv'),
       (2, 'tv short'),
       (3, 'movie'),
       (4, 'special'),
       (5, 'ova'),
       (6, 'ona');

-- drop table if exists magnets.show cascade;

create table magnets.show (
    show_id bigserial primary key,
    anilist_id bigint not null unique,
    season int,
    show_format int not null references magnets.show_format(show_format),
    created timestamptz not null default now()
);

create index on magnets.show(anilist_id);

create index on magnets.show(season);

-- drop table if exists magnets.show_name_type cascade;

create table magnets.show_name_type (
    show_name_type int primary key,
    description text not null,
    created timestamptz not null default now()
);

insert into magnets.show_name_type values (1, 'romaji'), (2, 'english'), (3, 'additional');

-- drop table if exists magnets.show_name cascade;

create table magnets.show_name (
    show_name_id bigserial primary key,
    show_id bigint not null references magnets.show,
    show_name_type int not null references magnets.show_name_type,
    name text not null,
    created timestamptz not null default now()
);

create index on magnets.show_name(show_id);

-- truncate magnets.show cascade;

-- drop table if exists magnets.schedule;

create table magnets.schedule (
    schedule_id bigserial primary key,
    show_id bigint not null references magnets.show,
    episode int not null,
    airs_at timestamptz not null,
    created timestamptz not null default now()
);

-- drop table if exists magnets.hash_type;

create table magnets.hash_type (
    hash_type int primary key,
    description text not null,
    created timestamptz not null default now()
);

insert into magnets.hash_type (hash_type, description) values (1, 'sha1');

-- drop table if exists magnets.torrent cascade;

create table magnets.torrent (
    torrent_id bigserial primary key,
    nyaa_id bigint not null unique,
    hash bytea not null,
    hash_type int not null references magnets.hash_type,
    uploaded_at timestamptz not null,
    title text not null,
    size bigint not null,
    matched bool not null default false,
    trusted bool not null,
    created timestamptz not null default now(),
    unique (hash, hash_type)
);

create index on magnets.torrent (nyaa_id desc) where matched;

create index on magnets.torrent (nyaa_id desc) where not matched;

-- drop table if exists magnets.rel_torrent_show cascade;

create table magnets.rel_torrent_show (
    rel_torrent_show_id bigserial primary key,
    show_id bigint not null references magnets.show,
    torrent_id bigint not null references magnets.torrent,
    nyaa_id bigint not null references magnets.torrent(nyaa_id),
    created timestamptz not null default now(),
    unique (torrent_id, show_id)
);

create index on magnets.rel_torrent_show (show_id, nyaa_id desc);

create table magnets.state (
    key text primary key,
    value jsonb not null,
    created timestamptz not null default now()
);

insert into magnets.state values
    ('max_nyaa_si_id', '0'::jsonb),
    ('last_schedule_update', '"2000-01-01T00:00:00Z"'::jsonb),
    ('last_shows_update', '"2000-01-01T00:00:00Z"'::jsonb),
    ('rematch_unmatched', '0'::jsonb),
    ('initial_setup', 'true'::jsonb);

create or replace procedure magnets.notify_state_change(key text) as $$
begin
    perform pg_notify('state_change', key);
    return;
end;
$$ language plpgsql;

create or replace function magnets.handle_state_update () returns trigger as $$
begin
    if NEW.key = 'max_nyaa_si_id' then
        if NEW.value::bigint < OLD.value::bigint then
            call magnets.notify_state_change(NEW.key);
        end if;
    elsif NEW.key = 'rematch_unmatched' then
        if NEW.value::int > 0 then
            call magnets.notify_state_change(NEW.key);
        end if;
    elsif NEW.key in ('last_shows_update', 'last_schedule_update') then
        if NEW.value::text::timestamptz < OLD.value::text::timestamptz then
            call magnets.notify_state_change(NEW.key);
        end if;
    end if;
    return null;
end;
$$ language plpgsql;

create trigger on_state_update
    after update on magnets.state
    for each row
    execute function magnets.handle_state_update();

commit;
