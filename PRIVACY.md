# Privacy

Blurlist exists to keep children's faces from being published without consent. It is built so that using it
does not create a new privacy risk.

## What Blurlist stores

Everything is kept in one SQLite database, `blurlist.sqlite3`, in the app's data folder (shown in
**Settings → About**):

| Data | Why | How long |
| --- | --- | --- |
| Roster: names, groups, notes | To show who matched | Until you remove the person |
| A face crop from each reference photo and its face embedding (a list of numbers) | To recognise the person | Until you remove the photo or person |
| For each scanned event: the folder path, file names, a small preview of each photo, a crop and an embedding of each detected face, and your decisions | So you can review, re-match and export later | Until you remove the event in **Scan** |
| Settings | Your preferences | Until you change them |

The original reference photos are not copied; only the face crop is kept.

## What Blurlist sends

Nothing. The app contains no networking code: no accounts, no cloud processing, no analytics, crash reporting
or update checks. The interface's content security policy blocks outgoing connections. The only time Blurlist
opens anything on the internet is if you click a model's *Source* link in Settings, which opens your browser.

## What Blurlist writes when exporting

A new folder containing re-encoded copies of the photos. Every copy is written from pixels only, so no EXIF,
GPS, XMP, IPTC, maker notes, comments or embedded thumbnails are carried over. The ICC colour profile is kept
because it describes colours, not people or places. No list of names or blurred faces is written into the
export folder.

## Removing data

- Remove a person in **Roster** to delete their name, face crops and embeddings.
- Remove an event in **Scan** to delete its previews, face crops, embeddings and decisions.
- Delete the data folder to remove everything.

## Recommendations for organisations

- Keep the computer running Blurlist encrypted (BitLocker, FileVault, LUKS) and password-protected, because the
  database contains face crops of children.
- Remove events once their photos are published.
- Treat Blurlist as an aid: have a person review the matches and the *Check these* pile before publishing.
