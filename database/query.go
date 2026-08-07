package database

// func InsertSongFTS(
// 	ctx context.Context,
// 	tx *sql.Tx,
// 	songID int64,
// 	name string,
// 	artists []string,
// ) error {
// 	_, err := tx.ExecContext(ctx, `
// 			INSERT INTO songs_fts(
// 					rowid,
// 					name,
// 					artists
// 			)
// 			VALUES (?, ?, ?)
// 	`,
// 		songID,
// 		name,
// 		strings.Join(artists, ", "),
// 	)

// 	return err
// }

// func DeleteSongFTS(
// 	ctx context.Context,
// 	tx *sql.Tx,
// 	songID int64,
// ) error {
// 	_, err := tx.ExecContext(ctx, `
// 			DELETE FROM songs_fts
// 			WHERE rowid = ?
// 	`, songID)
// 	return err
// }

// func UpdateSongFTS(
// 	ctx context.Context,
// 	tx *sql.Tx,
// 	songID int64,
// 	name string,
// 	artists []string,
// ) error {
// 	if err := DeleteSongFTS(ctx, tx, songID); err != nil {
// 		return err
// 	}
// 	return InsertSongFTS(ctx, tx, songID, name, artists)
// }

// func SearchSongs(
// 	ctx context.Context,
// 	db *sql.DB,
// 	keyword string,
// 	offset int64,
// 	limit int64,
// ) ([]repo.ListSongsRow, error) {
// 	query := `
// 			SELECT
// 					s.id,
// 					s.name,
// 					f.artists,
// 					s.size
// 			FROM songs_fts f
// 			JOIN songs s
// 			ON s.id = f.rowid
// 			WHERE songs_fts MATCH ?
// 			ORDER BY bm25(songs_fts)
// 			LIMIT ? OFFSET ?
// 	`

// 	rows, err := db.QueryContext(
// 		ctx,
// 		query,
// 		normalizeKeyword(keyword),
// 		limit,
// 		offset,
// 	)
// 	if err != nil {
// 		return nil, err
// 	}
// 	defer rows.Close()

// 	var results []repo.ListSongsRow
// 	for rows.Next() {
// 		var song repo.ListSongsRow
// 		err := rows.Scan(
// 			&song.ID,
// 			&song.Name,
// 			&song.Artists,
// 			&song.Size,
// 		)

// 		if err != nil {
// 			return nil, err
// 		}
// 		results = append(results, song)
// 	}

// 	if err := rows.Err(); err != nil {
// 		return nil, err
// 	}

// 	return results, nil
// }

// func normalizeKeyword(s string) string {
// 	words := strings.Fields(s)
// 	return strings.Join(words, " AND ")
// }
