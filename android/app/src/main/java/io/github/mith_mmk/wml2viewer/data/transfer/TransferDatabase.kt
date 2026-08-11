package io.github.mith_mmk.wml2viewer.data.transfer

import android.content.Context
import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase
import androidx.room.TypeConverters
import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

@Database(entities = [TransferJobV1::class], version = 2, exportSchema = true)
@TypeConverters(TransferJobConverters::class)
abstract class TransferDatabase : RoomDatabase() {
    abstract fun transferJobs(): TransferJobDao

    companion object {
        const val DATABASE_NAME = "transfer-jobs-v1.db"

        fun build(context: Context): TransferDatabase = Room.databaseBuilder(
            context.applicationContext,
            TransferDatabase::class.java,
            DATABASE_NAME,
        ).addMigrations(MIGRATION_1_2).build()

        internal val MIGRATION_1_2 = object : Migration(1, 2) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL("ALTER TABLE transfer_jobs_v1 ADD COLUMN planned_final_name TEXT")
                db.execSQL("ALTER TABLE transfer_jobs_v1 ADD COLUMN replacement_backup_name TEXT")
            }
        }
    }
}
