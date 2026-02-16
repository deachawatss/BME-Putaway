import { Injectable, inject } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { Observable, BehaviorSubject } from 'rxjs';
import { map, tap } from 'rxjs/operators';
import { environment } from '../../environments/environment';

// Models for ingredient intelligence system
export interface IngredientCompletionStatus {
  AllCompleted: null;
  PartiallyPicked: null;
  Unpicked: null;
}

export interface IngredientBatchStatus {
  item_key: string;
  line_id: number;
  description: string;
  total_batches: number;
  completed_batches: number;
  in_progress_batches: number;
  unpicked_batches: number;
  status: 'AllCompleted' | 'PartiallyPicked' | 'Unpicked';
  pack_size: string;
  completion_percentage: number;
}

export interface IngredientSwitchConfig {
  switch_threshold: number;
  switch_mode: 'Consecutive' | 'Total' | 'UserPreference';
  fallback_to_manual: boolean;
  ingredient_priority: number[];
}

export interface IngredientSwitchDecision {
  should_switch: boolean;
  current_ingredient: string;
  next_ingredient?: string;
  switch_reason: string;
  consecutive_completed: number;
  total_completed: number;
  remaining_ingredients: string[];
}

export interface RunCoordinationState {
  run_no: number;
  total_ingredients: number;
  ingredient_statuses: { [key: string]: IngredientBatchStatus };
  current_ingredient: string;
  consecutive_completed_batches: number;
  switch_config: IngredientSwitchConfig;
  last_switch_timestamp?: string;
}

export interface CrossIngredientLotOptimization {
  ingredient_lot_assignments: { [key: string]: string };
  lot_ingredient_usage: { [key: string]: string[] };
  pallet_sequence_per_ingredient: { [key: string]: number };
  lot_zone_preferences: { [key: string]: string };
}

export interface BatchCompletionRequest {
  batch_number: string;
  ingredient: string;
  line_id: number;
  picked_quantity: string;
  user_id: string;
}

export interface AutoSwitchResponse {
  coordination_state: RunCoordinationState;
  switch_decision: IngredientSwitchDecision;
}

export interface RunCompletionMetrics {
  run_no: number;
  total_ingredients: number;
  completed_ingredients: number;
  partially_picked_ingredients: number;
  unpicked_ingredients: number;
  overall_completion_percentage: number;
  total_batches: number;
  completed_batches: number;
  batch_completion_percentage: number;
  ingredient_details: IngredientBatchStatus[];
}

export interface ApiResponse<T> {
  success: boolean;
  data: T | null;
  message: string;
}

@Injectable({
  providedIn: 'root'
})
export class IngredientIntelligenceService {
  private readonly http = inject(HttpClient);
  private readonly baseUrl = `${environment.apiUrl}/runs`;

  // State management for current run coordination
  private coordinationStateSubject = new BehaviorSubject<RunCoordinationState | null>(null);
  public coordinationState$ = this.coordinationStateSubject.asObservable();

  private ingredientStatusesSubject = new BehaviorSubject<IngredientBatchStatus[]>([]);
  public ingredientStatuses$ = this.ingredientStatusesSubject.asObservable();

  /**
   * Get ingredient completion statuses for a run
   * Used for dashboard progress tracking and run overview
   */
  getIngredientStatuses(runNo: number): Observable<IngredientBatchStatus[]> {
    return this.http.get<ApiResponse<IngredientBatchStatus[]>>(`${this.baseUrl}/${runNo}/ingredient-statuses`)
      .pipe(
        map(response => {
          if (response.success && response.data) {
            return response.data;
          }
          throw new Error(response.message || 'Failed to get ingredient statuses');
        }),
        tap(statuses => {
          this.ingredientStatusesSubject.next(statuses);
        })
      );
  }

  /**
   * Get filtered ingredients for ItemKey search modal
   * Intelligently hides completed ingredients to reduce cognitive load
   */
  getAvailableIngredientsForSearch(runNo: number, hideCompleted: boolean = true): Observable<any[]> {
    const params = hideCompleted ? '?hide_completed=true' : '?hide_completed=false';
    return this.http.get<ApiResponse<any[]>>(`${this.baseUrl}/${runNo}/available-ingredients${params}`)
      .pipe(
        map(response => {
          if (response.success && response.data) {
            return response.data;
          }
          throw new Error(response.message || 'Failed to get available ingredients');
        }),
        tap(ingredients => {
        })
      );
  }

  /**
   * Initialize run coordination state for intelligent auto-switching
   * Sets up the coordination state for multi-ingredient workflow management
   */
  initializeRunCoordination(runNo: number): Observable<RunCoordinationState> {
    return this.http.post<ApiResponse<RunCoordinationState>>(`${this.baseUrl}/${runNo}/coordination/initialize`, {})
      .pipe(
        map(response => {
          if (response.success && response.data) {
            return response.data;
          }
          throw new Error(response.message || 'Failed to initialize run coordination');
        }),
        tap(coordinationState => {
          this.coordinationStateSubject.next(coordinationState);
        })
      );
  }

  /**
   * Evaluate auto-switching decision after batch completion
   * Core intelligence system for BME4-compatible ingredient switching
   */
  evaluateAutoSwitch(runNo: number, completionRequest: BatchCompletionRequest): Observable<AutoSwitchResponse> {
    return this.http.post<ApiResponse<AutoSwitchResponse>>(`${this.baseUrl}/${runNo}/coordination/evaluate-switch`, completionRequest)
      .pipe(
        map(response => {
          if (response.success && response.data) {
            return response.data;
          }
          throw new Error(response.message || 'Failed to evaluate auto-switch');
        }),
        tap(switchResponse => {
          // Update coordination state
          this.coordinationStateSubject.next(switchResponse.coordination_state);
          
          if (switchResponse.switch_decision.should_switch) {
          } else {
          }
        })
      );
  }

  /**
   * Get cross-ingredient lot optimization recommendations
   * Provides intelligent lot selection across multiple ingredients
   */
  getLotOptimization(runNo: number): Observable<CrossIngredientLotOptimization> {
    return this.http.get<ApiResponse<CrossIngredientLotOptimization>>(`${this.baseUrl}/${runNo}/lot-optimization`)
      .pipe(
        map(response => {
          if (response.success && response.data) {
            return response.data;
          }
          throw new Error(response.message || 'Failed to get lot optimization');
        }),
        tap(optimization => {
        })
      );
  }

  /**
   * Get next recommended ingredient based on workflow intelligence
   * Provides smart ingredient selection for operators
   */
  getNextRecommendedIngredient(runNo: number): Observable<string | null> {
    return this.http.get<ApiResponse<string | null>>(`${this.baseUrl}/${runNo}/next-ingredient`)
      .pipe(
        map(response => {
          if (response.success) {
            return response.data;
          }
          throw new Error(response.message || 'Failed to get next recommended ingredient');
        }),
        tap(nextIngredient => {
          if (nextIngredient) {
          } else {
          }
        })
      );
  }

  /**
   * Get comprehensive run completion metrics
   * Dashboard-level metrics for run progress tracking
   */
  getRunCompletionMetrics(runNo: number): Observable<RunCompletionMetrics> {
    return this.http.get<ApiResponse<RunCompletionMetrics>>(`${this.baseUrl}/${runNo}/completion-metrics`)
      .pipe(
        map(response => {
          if (response.success && response.data) {
            return response.data;
          }
          throw new Error(response.message || 'Failed to get run completion metrics');
        }),
        tap(metrics => {
        })
      );
  }

  /**
   * Check if ingredient should be hidden from ItemKey search
   * Used for dynamic filtering of completed ingredients
   */
  shouldHideIngredientFromSearch(status: IngredientBatchStatus): boolean {
    return status.status === 'AllCompleted';
  }

  /**
   * Calculate ingredient completion percentage
   * Helper for progress visualization
   */
  getIngredientCompletionPercentage(status: IngredientBatchStatus): number {
    return status.completion_percentage;
  }

  /**
   * Get ingredient status display text
   * Helper for UI status indicators
   */
  getIngredientStatusText(status: IngredientBatchStatus): string {
    switch (status.status) {
      case 'AllCompleted':
        return `✅ Complete (${status.completed_batches}/${status.total_batches})`;
      case 'PartiallyPicked':
        return `🔄 In Progress (${status.completed_batches}/${status.total_batches})`;
      case 'Unpicked':
        return `⏳ Pending (0/${status.total_batches})`;
      default:
        return 'Unknown';
    }
  }

  /**
   * Get current coordination state
   * Helper for accessing current state synchronously
   */
  getCurrentCoordinationState(): RunCoordinationState | null {
    return this.coordinationStateSubject.value;
  }

  /**
   * Get current ingredient statuses
   * Helper for accessing current statuses synchronously
   */
  getCurrentIngredientStatuses(): IngredientBatchStatus[] {
    return this.ingredientStatusesSubject.value;
  }

  /**
   * Determine if auto-switching should occur based on threshold
   * Helper for UI logic and decision making
   */
  shouldTriggerAutoSwitch(consecutiveCompleted: number, threshold: number = 3): boolean {
    return consecutiveCompleted >= threshold;
  }

  /**
   * Get ingredients available for switching (not completed)
   * Helper for manual ingredient selection
   */
  getAvailableIngredientsForSwitching(): IngredientBatchStatus[] {
    return this.ingredientStatusesSubject.value
      .filter(status => status.status !== 'AllCompleted');
  }

  /**
   * Get completion summary for dashboard display
   * Helper for dashboard metrics
   */
  getCompletionSummary(): { completed: number; total: number; percentage: number } {
    const statuses = this.ingredientStatusesSubject.value;
    const completed = statuses.filter(s => s.status === 'AllCompleted').length;
    const total = statuses.length;
    const percentage = total > 0 ? (completed / total) * 100 : 0;
    
    return { completed, total, percentage };
  }
}