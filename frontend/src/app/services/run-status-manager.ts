import { Injectable, signal } from '@angular/core';
import { BulkRunsService } from './bulk-runs.service';

/**
 * Centralized Run Status Manager
 *
 * Handles ALL run status transitions and completion logic in one place.
 * Eliminates race conditions and provides consistent behavior across the application.
 */

export enum StatusTrigger {
  AFTER_PICK = 'after_pick',
  PALLET_COMPLETED = 'pallet_completed',
  INGREDIENT_COMPLETED = 'ingredient_completed',
  RUN_COMPLETED = 'run_completed',
  MANUAL_CHECK = 'manual_check'
}

export interface RunStatusState {
  runNumber: number;
  status: string;
  lastUpdated: Date;
}

@Injectable({
  providedIn: 'root'
})
export class RunStatusManager {
  // Race condition protection
  private completionCheckInProgress = false;
  private lastCompletionCheckTimestamp = 0;
  private completionCheckTimeout: any = null;
  private readonly COMPLETION_DEBOUNCE_MS = 1000;

  // Status state management
  private currentRunStatus = signal<RunStatusState | null>(null);

  constructor(private bulkRunsService: BulkRunsService) {}

  /**
   * Single entry point for all completion checks
   * Replaces all scattered checkAndUpdateRunCompletion() calls
   */
  public triggerCompletionCheck(runNumber: number, trigger: StatusTrigger): void {
    // **STATUS GUARD**: Skip if status is already PRINT
    const currentStatus = this.currentRunStatus()?.status;
    if (currentStatus === 'PRINT') {
      return;
    }

    // **RACE CONDITION PROTECTION**: Implement debouncing and mutex
    const currentTime = Date.now();

    // Clear existing timeout if present
    if (this.completionCheckTimeout) {
      clearTimeout(this.completionCheckTimeout);
      this.completionCheckTimeout = null;
    }

    // Check debounce period
    if (currentTime - this.lastCompletionCheckTimestamp < this.COMPLETION_DEBOUNCE_MS) {
      return;
    }

    // Check mutex
    if (this.completionCheckInProgress) {
      return;
    }

    // Execute with debounce delay
    this.completionCheckTimeout = setTimeout(() => {
      this.executeCompletionCheck(runNumber, trigger);
    }, 100); // Minimal delay for UI consistency
  }

  /**
   * Execute the actual completion check - Protected by mutex
   */
  private executeCompletionCheck(runNumber: number, trigger: StatusTrigger): void {
    // **DOUBLE STATUS GUARD**: Verify status before execution
    const currentStatus = this.currentRunStatus()?.status;
    if (currentStatus === 'PRINT') {
      return;
    }

    // **MUTEX CHECK**: Ensure no concurrent execution
    if (this.completionCheckInProgress) {
      return;
    }

    // Set mutex and update timestamp
    this.completionCheckInProgress = true;
    this.lastCompletionCheckTimestamp = Date.now();

    // Call backend to check completion
    this.bulkRunsService.checkDetailedRunCompletion(runNumber).subscribe({
      next: (response: any) => {
        try {
          if (response.success && response.data) {
            const { is_complete, incomplete_count, completed_count, total_ingredients } = response.data;

            if (is_complete) {
              this.updateRunStatusToPrint(runNumber);
            }
          }
        } finally {
          // Always release mutex
          this.completionCheckInProgress = false;
        }
      },
      error: (error: any) => {
        console.error(`❌ COMPLETION_CHECK_ERROR: Failed to check run ${runNumber} completion:`, error);
        // Always release mutex on error
        this.completionCheckInProgress = false;
      }
    });
  }

  /**
   * Update run status from NEW to PRINT
   * Centralized status update with consistent error handling
   */
  private updateRunStatusToPrint(runNumber: number): void {
    // **FINAL STATUS GUARD**: Last check before API call
    const currentStatus = this.currentRunStatus()?.status;
    if (currentStatus === 'PRINT') {
      this.showCompletionMessage(runNumber, 'Status is already PRINT');
      return;
    }

    this.bulkRunsService.updateRunStatusToPrint(runNumber).subscribe({
      next: (response: any) => {
        if (response.success) {
          // Update local status state
          this.updateLocalStatus(runNumber, 'PRINT');

          // Show success message
          this.showCompletionMessage(runNumber, 'Status successfully updated to PRINT');
        }
      },
      error: (error: any) => {
        // **SMART ERROR HANDLING**: Check if error is "already PRINT"
        const errorMessage = error?.error?.message || error?.message || error.toString();

        if (errorMessage.includes('already') && errorMessage.includes('PRINT')) {
          // Update local status to sync
          this.updateLocalStatus(runNumber, 'PRINT');

          // Show completion message
          this.showCompletionMessage(runNumber, 'Status was already PRINT');
        } else {
          console.error(`Failed to update run ${runNumber} status:`, error);
        }
      }
    });
  }

  /**
   * Update local status state
   */
  private updateLocalStatus(runNumber: number, status: string): void {
    this.currentRunStatus.set({
      runNumber,
      status,
      lastUpdated: new Date()
    });
  }

  /**
   * Show completion message to user
   */
  private showCompletionMessage(runNumber: number, details: string): void {
    const message = `🎉 Congratulations! Run ${runNumber} is complete!\n\n${details}. All ingredients have been successfully picked.`;
    alert(message);
  }

  /**
   * Public method to refresh run status (called from component)
   * Component should call this after getting fresh status from backend
   */
  public refreshRunStatus(runNumber?: number, newStatus?: string): void {
    if (runNumber) {
      if (newStatus) {
        this.updateLocalStatus(runNumber, newStatus);
      }
      // Let component handle the actual backend call
    }
  }

  /**
   * Get current run status (for component access)
   */
  public getCurrentStatus(): RunStatusState | null {
    return this.currentRunStatus();
  }

  /**
   * Set current run status (called when component loads run data)
   */
  public setCurrentStatus(runNumber: number, status: string): void {
    this.updateLocalStatus(runNumber, status);
  }

  /**
   * Reset manager state (for testing or component cleanup)
   */
  public reset(): void {
    this.completionCheckInProgress = false;
    this.lastCompletionCheckTimestamp = 0;
    if (this.completionCheckTimeout) {
      clearTimeout(this.completionCheckTimeout);
      this.completionCheckTimeout = null;
    }
    this.currentRunStatus.set(null);
  }
}